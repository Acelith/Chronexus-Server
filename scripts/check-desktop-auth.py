#!/usr/bin/env python3
"""Exercise the original desktop Rust HTTP client against a real local server.

Uses a temporary test crate; never edits the app or reads its user profile.
Only the domain validators unrelated to authentication are small harness stubs.
Requires the desktop source checkout and its cached reqwest dependencies.
"""
import argparse
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE = r'''
mod journal {
    pub type AppResult<T> = Result<T, String>;
    pub fn identifier(value: &str) -> AppResult<()> {
        uuid::Uuid::parse_str(value).map(|_|()).map_err(|_|"Invalid UUID".into())
    }
}
#[allow(dead_code)]
mod desktop {
    pub const API_VERSION: u32 = 1;
    pub fn remote_id(value: &str) -> Result<(),String> {
        if value.is_empty() || value.len()>64 || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)) { Err("Invalid ID".into()) } else { Ok(()) }
    }
    pub fn kind(value: &str) -> Result<(),String> {
        if ["page","session","relationship","party"].contains(&value) { Ok(()) } else { Err("Invalid kind".into()) }
    }
    #[path = __CLIENT__]
    pub mod api;
}
#[tokio::test]
async fn original_desktop_meta_redeem_login_logout() {
    use chronexus_server::{AppState,config::Config,routes};
    let directory=tempfile::tempdir_in(__TEMPDIR__).unwrap();
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address=listener.local_addr().unwrap();let origin=format!("http://{address}");
    let config=Config::parse(std::collections::HashMap::from([
        ("CHRONEXUS_PUBLIC_ORIGIN".into(),origin.clone()),
        ("CHRONEXUS_ALLOW_LOOPBACK_HTTP".into(),"true".into()),
        ("CHRONEXUS_LISTEN".into(),address.to_string()),
        ("CHRONEXUS_DATABASE".into(),directory.path().join("qa.sqlite3").display().to_string())
    ])).unwrap();
    let state=AppState::new(config).unwrap();let invite=state.database.issue_invite(None).unwrap();
    let app=routes::router(state.clone());
    let task=tokio::spawn(async move { axum::serve(listener,app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap(); });
    let client=desktop::api::Client::new(url::Url::parse(&origin).unwrap(),None,true).unwrap();
    assert_eq!(client.meta().await.unwrap().version,1);
    let first=client.redeem(&invite,"qa_player","QA Player","synthetic-password-123","QA desktop").await.unwrap();
    assert_eq!(first.username,"qa_player");assert_eq!(first.display_name,"QA Player");
    assert!(client.redeem(&invite,"qa_other","QA Other","synthetic-password-123","QA desktop").await.is_err());
    let second=client.login("qa_player","synthetic-password-123","QA laptop").await.unwrap();
    assert_eq!(first.account_id,second.account_id);assert_ne!(first.token,second.token);
    let authenticated=desktop::api::Client::new(url::Url::parse(&origin).unwrap(),Some(first.token.clone()),true).unwrap();
    let campaign=authenticated.create_campaign("Synthetic campaign").await.unwrap();
    assert_eq!(campaign.role,"owner");
    let invite=authenticated.invite(&campaign.id,"reader").await.unwrap();
    let reader=client.redeem(&invite.code,"qa_reader","QA Reader","synthetic-password-123","QA reader").await.unwrap();
    let reader_client=desktop::api::Client::new(url::Url::parse(&origin).unwrap(),Some(reader.token.clone()),true).unwrap();
    assert!(reader_client.invite(&campaign.id,"player").await.is_err());
    let server_invite=state.database.issue_invite(None).unwrap();
    let player=client.redeem(&server_invite,"qa_player_two","QA Player Two","synthetic-password-123","QA player").await.unwrap();
    let player_client=desktop::api::Client::new(url::Url::parse(&origin).unwrap(),Some(player.token.clone()),true).unwrap();
    let invite=authenticated.invite(&campaign.id,"player").await.unwrap();
    assert_eq!(player_client.join(&invite.code).await.unwrap().role,"player");
    let first_page=uuid::Uuid::new_v4().to_string();
    let second_page=uuid::Uuid::new_v4().to_string();
    let document=serde_json::json!({"version":1,"blocks":[{"id":"block-1","type":"paragraph","props":{},"content":[],"children":[]}]});
    let page=serde_json::json!({"title":"Public synthetic page","kind":"npc","status":"active","aliases":[],"document":document});
    for id in [&first_page,&second_page] {assert!(matches!(authenticated.put_item(&campaign.id,"page",id,0,&page).await.unwrap(),desktop::api::Write::Accepted{revision:1}));}
    let session=serde_json::json!({"sequence":1,"title":"Session one","playedOn":null,"document":document});
    let session_id=uuid::Uuid::new_v4().to_string();
    authenticated.put_item(&campaign.id,"session",&session_id,0,&session).await.unwrap();
    let relationship=serde_json::json!({"fromPageId":first_page,"toPageId":second_page,"label":"Knows","certainty":"suspected","evidence":"Player claim","notes":""});
    authenticated.put_item(&campaign.id,"relationship",&uuid::Uuid::new_v4().to_string(),0,&relationship).await.unwrap();
    let party=serde_json::json!({"pageId":first_page,"name":"Synthetic party member","playerName":"QA","role":"Scout","status":"active"});
    authenticated.put_item(&campaign.id,"party",&first_page,0,&party).await.unwrap();
    let changes=reader_client.changes(&campaign.id,"").await.unwrap();assert_eq!(changes.items.len(),5);
    assert!(changes.items.iter().all(|item|item.author_id==first.account_id));
    assert!(matches!(authenticated.put_item(&campaign.id,"page",&first_page,0,&page).await.unwrap(),desktop::api::Write::Conflict{revision:1,..}));
    assert!(reader_client.put_item(&campaign.id,"page",&uuid::Uuid::new_v4().to_string(),0,&page).await.is_err());
    assert!(player_client.put_item(&campaign.id,"page",&first_page,1,&page).await.is_err());
    authenticated.delete_item(&campaign.id,"page",&first_page,1).await.unwrap();
    let removed=reader_client.changes(&campaign.id,&changes.cursor).await.unwrap();
    assert_eq!(removed.items.len(),3);assert!(removed.items.iter().all(|item|item.deleted&&item.payload.is_none()));
    authenticated.logout().await.unwrap();assert!(state.database.sessions(&first.token).is_err());
    assert_eq!(state.database.sessions(&second.token).unwrap().len(),1);
    task.abort();println!("Original desktop auth, campaign and sharing client: PASS (real loopback HTTP, synthetic profile).");
}
'''


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--app-dir", required=True, type=Path)
    args = parser.parse_args()
    client = args.app_dir.resolve() / "src-tauri/src/sync/api.rs"
    if not client.is_file():
        raise SystemExit("Desktop sync/api.rs was not found")
    tempbase = "/private/tmp" if Path("/private/tmp").is_dir() else tempfile.gettempdir()
    with tempfile.TemporaryDirectory(prefix="chronexus-auth-contract-", dir=tempbase) as tmp:
        project = Path(tmp)
        (project / "src").mkdir()
        manifest = f'''[package]
name = "chronexus-desktop-auth-contract"
version = "0.0.0"
edition = "2024"
[dependencies]
chronexus-server = {{ path = {json.dumps(str(ROOT))} }}
axum = "0.8"
tokio = {{ version = "1", features = ["macros", "rt-multi-thread", "net"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
url = "2"
uuid = "1"
tempfile = "3"
reqwest = {{ version = "=0.13.5", default-features = false, features = ["json", "stream", "rustls"] }}
'''
        (project / "Cargo.toml").write_text(manifest)
        shutil.copyfile(ROOT / "Cargo.lock", project / "Cargo.lock")
        (project / "src/lib.rs").write_text(SOURCE.replace("__CLIENT__", json.dumps(str(client))).replace("__TEMPDIR__", json.dumps(tempbase)))
        env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target/desktop-contract"))
        subprocess.run(["cargo", "test", "--offline", "--", "--nocapture"], cwd=project, env=env, check=True)


if __name__ == "__main__":
    main()
