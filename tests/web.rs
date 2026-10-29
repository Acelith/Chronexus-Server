//! M25.11: web interface pages and the read-only overviews behind them.
mod common;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use chronexus_server::{AppState, config::Config, routes};
use common::{Fixture, call};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr};
use tower::ServiceExt;

/// A fixture whose configuration names `operator` as the operator, served on `origin`.
fn with(origin: &str, operators: &str) -> Fixture {
    let directory = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    let config = Config::parse(HashMap::from([
        (
            "CHRONEXUS_DATABASE".into(),
            directory
                .path()
                .join("server.sqlite3")
                .display()
                .to_string(),
        ),
        ("CHRONEXUS_PUBLIC_ORIGIN".into(), origin.into()),
        ("CHRONEXUS_ALLOW_LOOPBACK_HTTP".into(), "true".into()),
        ("CHRONEXUS_OPERATORS".into(), operators.into()),
    ]))
    .unwrap();
    let state = AppState::new(config).unwrap();
    let app = routes::router(state.clone());
    Fixture {
        _directory: directory,
        state,
        app,
    }
}
async fn raw(f: &Fixture, path: &str, host: &str) -> (StatusCode, HashMap<String, String>, String) {
    let mut request = Request::builder()
        .uri(path)
        .header("host", host)
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:50000".parse::<SocketAddr>().unwrap(),
    ));
    let response = f.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}
fn token(account: &Value) -> String {
    account["token"].as_str().unwrap().to_string()
}
async fn campaign(f: &Fixture, owner: &str, name: &str) -> String {
    let (status, body) = f
        .call(
            "POST",
            "/api/v1/campaigns",
            json!({"name": name}),
            Some(owner),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_string()
}
async fn join(f: &Fixture, owner: &str, campaign: &str, player: &str, role: &str) {
    let (_, invite) = f
        .call(
            "POST",
            &format!("/api/v1/campaigns/{campaign}/invites"),
            json!({"role": role}),
            Some(owner),
        )
        .await;
    let (status, body) = f
        .call(
            "POST",
            "/api/v1/campaigns/join",
            json!({"inviteCode": invite["code"]}),
            Some(player),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
async fn save(f: &Fixture, token: &str, campaign: &str, id: &str, visibility: &str, text: &str) {
    let payload = json!({"title":text,"kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"block-1","type":"paragraph","props":{},"content":[{"type":"text","text":text,"styles":{}}],"children":[]}]}});
    let (status, body) = f
        .call(
            "PUT",
            &format!("/api/v1/campaigns/{campaign}/content/page/{id}"),
            json!({"baseRevision": 0, "visibility": visibility, "payload": payload}),
            Some(token),
        )
        .await;
    assert!(status.is_success(), "{status} {body}");
}

#[tokio::test]
async fn pages_are_served_with_a_strict_policy_and_no_cookie() {
    let f = Fixture::new();
    let (status, headers, body) = raw(&f, "/", "127.0.0.1:8080").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers["content-type"].starts_with("text/html"));
    assert!(body.contains("/web/app.js"));
    let policy = &headers["content-security-policy"];
    assert!(policy.contains("script-src 'self'") && policy.contains("frame-ancestors 'none'"));
    assert!(!policy.contains("unsafe"), "{policy}");
    assert!(!headers.contains_key("set-cookie"));
    assert_eq!(headers["x-frame-options"], "DENY");
    let (status, headers, script) = raw(&f, "/web/app.js", "127.0.0.1:8080").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers["content-type"].starts_with("text/javascript"));
    assert!(
        !script.contains("innerHTML"),
        "untrusted text must never become HTML"
    );
    let (status, headers, translations) = raw(&f, "/web/i18n.js", "127.0.0.1:8080").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers["content-type"].starts_with("text/javascript"));
    assert!(translations.contains("ChronexusI18n") && !translations.contains("innerHTML"));
    assert!(body.find("/web/i18n.js") < body.find("/web/app.js"));
    let (status, _, _) = raw(&f, "/web/app.css", "127.0.0.1:8080").await;
    assert_eq!(status, StatusCode::OK);
    // The API keeps the policy that allows nothing.
    let (_, headers, _) = raw(&f, "/api/v1/meta", "127.0.0.1:8080").await;
    assert_eq!(
        headers["content-security-policy"],
        "default-src 'none'; frame-ancestors 'none'"
    );
    // The page is served only for the configured host, like the API.
    let (status, _, _) = raw(&f, "/", "evil.example").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn overview_shows_own_usage_and_never_another_members_private_data() {
    let f = with("http://127.0.0.1:8080", "");
    let owner = token(&f.account("owner").await);
    let player = token(&f.account("player").await);
    let outsider = token(&f.account("outsider").await);
    let id = campaign(&f, &owner, "Overview").await;
    join(&f, &owner, &id, &player, "player").await;
    save(
        &f,
        &owner,
        &id,
        "11111111-1111-4111-8111-111111111111",
        "private",
        "Owner secret",
    )
    .await;
    save(
        &f,
        &owner,
        &id,
        "22222222-2222-4222-8222-222222222222",
        "members",
        "Owner shared",
    )
    .await;
    save(
        &f,
        &player,
        &id,
        "33333333-3333-4333-8333-333333333333",
        "private",
        "Player secret",
    )
    .await;

    let (status, mine) = f
        .call(
            "GET",
            &format!("/api/v1/campaigns/{id}/overview"),
            Value::Null,
            Some(&player),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{mine}");
    assert_eq!(mine["role"], "player");
    assert_eq!(mine["members"], 2);
    assert_eq!(mine["myItems"][0]["kind"], "page");
    assert_eq!(mine["myItems"][0]["privateItems"], 1);
    assert_eq!(mine["myItems"][0]["sharedItems"], 0);
    assert_eq!(mine["sharedByOthers"], 1, "only what the owner shares");
    let text = mine.to_string();
    assert!(
        !text.contains("secret") && !text.contains("Owner shared"),
        "no content: {text}"
    );
    assert!(mine["myLastSavedAt"].is_string());
    assert_eq!(mine["quotas"]["campaignItems"], 5000);

    let (status, theirs) = f
        .call(
            "GET",
            &format!("/api/v1/campaigns/{id}/overview"),
            Value::Null,
            Some(&owner),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(theirs["myItems"][0]["privateItems"], 1);
    assert_eq!(theirs["myItems"][0]["sharedItems"], 1);
    assert_eq!(
        theirs["sharedByOthers"], 0,
        "the player's private page is invisible"
    );

    let (status, _) = f
        .call(
            "GET",
            &format!("/api/v1/campaigns/{id}/overview"),
            Value::Null,
            Some(&outsider),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = f
        .call(
            "GET",
            &format!("/api/v1/campaigns/{id}/overview"),
            Value::Null,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = f
        .call(
            "GET",
            "/api/v1/campaigns/not-a-uuid/overview",
            Value::Null,
            Some(&owner),
        )
        .await;
    assert!(status.is_client_error());
}

#[tokio::test]
async fn operator_sees_service_and_account_metadata_but_no_campaign() {
    let f = with("http://127.0.0.1:8080", "Operator, admin2");
    let operator = token(&f.account("operator").await);
    let owner = token(&f.account("owner").await);
    let reader = token(&f.account("reader").await);
    let id = campaign(&f, &owner, "Secret campaign name").await;
    join(&f, &owner, &id, &reader, "reader").await;
    save(
        &f,
        &owner,
        &id,
        "44444444-4444-4444-8444-444444444444",
        "members",
        "Hidden text",
    )
    .await;

    let (_, account) = f
        .call("GET", "/api/v1/account", Value::Null, Some(&operator))
        .await;
    assert_eq!(account["operator"], true);
    assert_eq!(account["username"], "operator");
    let (_, account) = f
        .call("GET", "/api/v1/account", Value::Null, Some(&owner))
        .await;
    assert_eq!(account["operator"], false);

    let (status, service) = f
        .call(
            "GET",
            "/api/v1/operator/status",
            Value::Null,
            Some(&operator),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{service}");
    assert_eq!(service["accounts"], 3);
    assert_eq!(service["campaigns"], 1);
    assert_eq!(service["contentItems"], 1);
    assert_eq!(service["aiService"], "not installed");
    let (status, accounts) = f
        .call(
            "GET",
            "/api/v1/operator/accounts",
            Value::Null,
            Some(&operator),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accounts.as_array().unwrap().len(), 3);
    let text = accounts.to_string() + &service.to_string();
    assert!(
        !text.contains("Secret campaign name") && !text.contains("Hidden text"),
        "{text}"
    );

    // The operator role opens nothing in campaigns it is not a member of.
    for path in [
        format!("/api/v1/campaigns/{id}"),
        format!("/api/v1/campaigns/{id}/overview"),
        format!("/api/v1/campaigns/{id}/members"),
        format!("/api/v1/campaigns/{id}/content"),
    ] {
        let (status, _) = f.call("GET", &path, Value::Null, Some(&operator)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
    // Owners, readers and anonymous callers get no operator data.
    for who in [Some(owner.as_str()), Some(reader.as_str())] {
        for path in ["/api/v1/operator/status", "/api/v1/operator/accounts"] {
            let (status, _) = f.call("GET", path, Value::Null, who).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        }
    }
    let (status, _) = f
        .call("GET", "/api/v1/operator/status", Value::Null, None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_ended_session_closes_the_web_view_and_an_alternate_port_works() {
    let f = with("http://127.0.0.1:18443", "");
    let code = f.state.database.issue_invite(None).unwrap();
    let (status, account) = call(
        f.app.clone(), "POST", "/api/v1/auth/redeem",
        json!({"inviteCode":code,"username":"web","displayName":"Web","password":"synthetic-password-123","device":"Web browser"}),
        None, "127.0.0.1:18443", Some("http://127.0.0.1:18443"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{account}");
    let web = token(&account);
    let (status, _, _) = raw(&f, "/", "127.0.0.1:18443").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        f.app.clone(),
        "GET",
        "/api/v1/account",
        Value::Null,
        Some(&web),
        "127.0.0.1:18443",
        Some("http://127.0.0.1:18443"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // A page from another origin cannot use the API, even with a valid token.
    let (status, _) = call(
        f.app.clone(),
        "GET",
        "/api/v1/account",
        Value::Null,
        Some(&web),
        "127.0.0.1:18443",
        Some("https://evil.example"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        f.app.clone(),
        "POST",
        "/api/v1/auth/logout",
        Value::Null,
        Some(&web),
        "127.0.0.1:18443",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        f.app.clone(),
        "GET",
        "/api/v1/account",
        Value::Null,
        Some(&web),
        "127.0.0.1:18443",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[test]
fn operator_names_are_validated() {
    let base = |ops: &str| {
        Config::parse(HashMap::from([
            (
                "CHRONEXUS_PUBLIC_ORIGIN".into(),
                "http://127.0.0.1:8080".into(),
            ),
            ("CHRONEXUS_ALLOW_LOOPBACK_HTTP".into(), "true".into()),
            ("CHRONEXUS_OPERATORS".into(), ops.into()),
        ]))
    };
    assert_eq!(
        base(" Alice , bob.ops ").unwrap().operators,
        ["alice", "bob.ops"]
    );
    assert!(base("").unwrap().operators.is_empty());
    assert!(base("a").is_err());
    assert!(base("bad name").is_err());
}
