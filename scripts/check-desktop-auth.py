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
    authenticated.logout().await.unwrap();assert!(state.database.sessions(&first.token).is_err());
    assert_eq!(state.database.sessions(&second.token).unwrap().len(),1);
    task.abort();println!("Original desktop auth client: PASS (real loopback HTTP, synthetic profile).");
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
