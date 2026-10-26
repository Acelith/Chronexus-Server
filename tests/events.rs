//! Notification streams: slots are released for slow readers and hints never reveal
//! activity a member is not allowed to see.
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use chronexus_server::{
    AppState,
    config::Config,
    content::{Visibility, WriteContent},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr, time::Duration};
use tower::ServiceExt;

async fn request(s: &AppState, path: &str, token: &str) -> axum::response::Response {
    let mut r = Request::builder()
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    r.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:50000".parse::<SocketAddr>().unwrap(),
    ));
    chronexus_server::routes::router(s.clone())
        .oneshot(r)
        .await
        .unwrap()
}
fn state() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::parse(HashMap::from([
        (
            "CHRONEXUS_DATABASE".into(),
            dir.path().join("server.sqlite3").display().to_string(),
        ),
        (
            "CHRONEXUS_PUBLIC_ORIGIN".into(),
            "http://127.0.0.1:8080".into(),
        ),
        ("CHRONEXUS_ALLOW_LOOPBACK_HTTP".into(), "true".into()),
    ]))
    .unwrap();
    (dir, AppState::new(config).unwrap())
}
fn account(s: &AppState, name: &str) -> String {
    let code = s.database.issue_invite(None).unwrap();
    let input = serde_json::from_value(json!({"inviteCode":code,"username":name,"displayName":name,"password":"synthetic-password-123","device":"test"})).unwrap();
    let value: Value =
        serde_json::to_value(s.database.redeem(&input, "127.0.0.1").unwrap()).unwrap();
    value["token"].as_str().unwrap().to_string()
}
fn campaign(s: &AppState, token: &str, name: &str) -> String {
    s.database
        .create_campaign(
            token,
            &serde_json::from_value(json!({ "name": name })).unwrap(),
        )
        .unwrap()
        .id
}
fn save(s: &AppState, token: &str, campaign: &str, id: &str, revision: i64, visibility: &str) {
    let input: WriteContent = serde_json::from_value(json!({"baseRevision":revision,"visibility":visibility,"payload":{"title":"Synthetic","kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"block-1","type":"paragraph","props":{},"content":[{"type":"text","text":"Synthetic","styles":{}}],"children":[]}]}}})).unwrap();
    assert_eq!(
        input.visibility == Visibility::Private,
        visibility == "private"
    );
    s.database
        .write_content(token, campaign, "page", id, &input, false)
        .unwrap();
}
async fn ready(body: &mut Body) {
    let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert!(frame.starts_with(b"event: ready"));
}
async fn quiet(body: &mut Body) -> bool {
    tokio::time::timeout(Duration::from_millis(500), body.frame())
        .await
        .is_err()
}
async fn changed(body: &mut Body) {
    let frame = tokio::time::timeout(Duration::from_secs(2), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert_eq!(&frame[..], b"event: changed\ndata: {}\n\n");
}

#[tokio::test(flavor = "current_thread")]
async fn a_lagging_stream_does_not_reveal_private_activity_elsewhere() {
    let (_dir, s) = state();
    let (alice, bob) = (account(&s, "alice"), account(&s, "bob"));
    let bobs = campaign(&s, &bob, "Bob only");
    let alices = campaign(&s, &alice, "Alice private");
    let mut body = request(&s, &format!("/api/v1/campaigns/{bobs}/events"), &bob)
        .await
        .into_body();
    ready(&mut body).await;
    // Enough private writes to overflow the broadcast buffer of Bob's stream.
    let id = uuid::Uuid::new_v4().to_string();
    for revision in 0..100 {
        save(&s, &alice, &alices, &id, revision, "private");
    }
    assert!(quiet(&mut body).await);
    // Bob's own change after a lag still reaches him.
    for revision in 0..100 {
        save(&s, &alice, &alices, &id, 100 + revision, "private");
    }
    save(
        &s,
        &bob,
        &bobs,
        &uuid::Uuid::new_v4().to_string(),
        0,
        "private",
    );
    changed(&mut body).await;
}

#[tokio::test(flavor = "current_thread")]
async fn deleting_a_never_shared_page_does_not_notify_other_members() {
    let (_dir, s) = state();
    let (alice, bob) = (account(&s, "author"), account(&s, "member"));
    let c = campaign(&s, &alice, "Shared campaign");
    let invite = s
        .database
        .campaign_invite(
            &alice,
            &c,
            &serde_json::from_value(json!({"role":"player"})).unwrap(),
        )
        .unwrap();
    s.database
        .join_campaign(
            &bob,
            &serde_json::from_value(json!({ "inviteCode": invite.code })).unwrap(),
            "127.0.0.1",
        )
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    save(&s, &alice, &c, &id, 0, "private");
    let mut body = request(&s, &format!("/api/v1/campaigns/{c}/events"), &bob)
        .await
        .into_body();
    ready(&mut body).await;
    s.database
        .delete_content(&alice, &c, "page", &id, 1, None, false)
        .unwrap();
    assert!(quiet(&mut body).await);
    // A shared page is still announced.
    save(
        &s,
        &alice,
        &c,
        &uuid::Uuid::new_v4().to_string(),
        0,
        "members",
    );
    changed(&mut body).await;
}

/// Advances paused time and lets the stream tasks and their blocking checks finish.
async fn settle(seconds: u64) {
    tokio::time::advance(Duration::from_secs(seconds)).await;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(2));
        tokio::task::yield_now().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unread_streams_of_a_revoked_session_release_their_slots() {
    let (_dir, s) = state();
    let token = account(&s, "slowreader");
    let c = campaign(&s, &token, "Synthetic");
    let path = format!("/api/v1/campaigns/{c}/events");
    let mut held = Vec::new();
    for _ in 0..3 {
        let response = request(&s, &path, &token).await;
        assert_eq!(response.status(), StatusCode::OK);
        held.push(response.into_body());
    }
    assert_eq!(
        request(&s, &path, &token).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    tokio::task::yield_now().await;
    tokio::time::pause();
    // Fill every queue: nobody reads these bodies.
    let id = uuid::Uuid::new_v4().to_string();
    for revision in 0..4 {
        save(&s, &token, &c, &id, revision, "private");
        tokio::task::yield_now().await;
    }
    // A keepalive meets the full queues while the session is still valid: the old code
    // waited there forever, so later revocation and lifetime checks never ran.
    settle(26).await;
    s.database.revoke(&token, None, true).unwrap();
    let login = serde_json::from_value(
        json!({"username":"slowreader","password":"synthetic-password-123","device":"new session"}),
    )
    .unwrap();
    let fresh = s.database.login(&login, "127.0.0.1", "").unwrap().token;
    settle(101).await;
    assert_eq!(request(&s, &path, &fresh).await.status(), StatusCode::OK);
    drop(held);
}
