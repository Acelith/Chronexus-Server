use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use chronexus_server::{AppState, auth, config::Config, routes};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr};
use tower::ServiceExt;

struct Fixture {
    _directory: tempfile::TempDir,
    state: AppState,
    app: Router,
}
impl Fixture {
    fn new() -> Self {
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
            (
                "CHRONEXUS_PUBLIC_ORIGIN".into(),
                "http://127.0.0.1:8080".into(),
            ),
            ("CHRONEXUS_ALLOW_LOOPBACK_HTTP".into(), "true".into()),
        ]))
        .unwrap();
        let state = AppState::new(config).unwrap();
        let app = routes::router(state.clone());
        Self {
            _directory: directory,
            state,
            app,
        }
    }
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Value,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        call(
            self.app.clone(),
            method,
            path,
            body,
            token,
            "127.0.0.1:8080",
            None,
        )
        .await
    }
    async fn account(&self, name: &str) -> Value {
        let code = self.state.database.issue_invite(None).unwrap();
        let (status,body)=self.call("POST","/api/v1/auth/redeem",json!({"inviteCode":code,"username":name,"displayName":name,"password":"synthetic-password-123","device":"test desktop"}),None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }
}
async fn call(
    app: Router,
    method: &str,
    path: &str,
    body: Value,
    token: Option<&str>,
    host: &str,
    origin: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", host)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    let mut request = request
        .body(Body::from(if body.is_null() {
            String::new()
        } else {
            body.to_string()
        }))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:50000".parse::<SocketAddr>().unwrap(),
    ));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn meta_health_and_contract_describe_only_implemented_scope() {
    let f = Fixture::new();
    let (status, meta) = f.call("GET", "/api/v1/meta", Value::Null, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(meta["api"], "chronexus-sync");
    assert_eq!(meta["version"], 1);
    assert_eq!(meta["capabilities"], json!(["accounts", "sessions"]));
    assert_eq!(
        f.call("GET", "/health/ready", Value::Null, None).await.0,
        StatusCode::OK
    );
    let (_, contract) = f
        .call("GET", "/api/v1/openapi.json", Value::Null, None)
        .await;
    assert!(contract["paths"]["/api/v1/auth/login"].is_object());
    assert!(contract["paths"]["/api/v1/campaigns"].is_null());
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/campaigns",
            json!({"name":"unimplemented"}),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn invitation_is_single_use_and_concurrent_redemption_is_atomic() {
    let f = Fixture::new();
    let code = f.state.database.issue_invite(None).unwrap();
    let payload = |name: &str| json!({"inviteCode":code,"username":name,"displayName":name,"password":"synthetic-password-123","device":"desktop"});
    let (one, two) = tokio::join!(
        f.call("POST", "/api/v1/auth/redeem", payload("first"), None),
        f.call("POST", "/api/v1/auth/redeem", payload("second"), None)
    );
    let mut codes = [one.0.as_u16(), two.0.as_u16()];
    codes.sort();
    assert_eq!(codes, [200, 410]);
    let count: i64 = f
        .state
        .database
        .with(|conn| Ok(conn.query_row("SELECT count(*) FROM accounts", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        f.call("POST", "/api/v1/auth/redeem", payload("third"), None)
            .await
            .0,
        StatusCode::GONE
    );
}

#[tokio::test]
async fn expired_invalid_and_recovery_codes_cannot_register() {
    let f = Fixture::new();
    f.account("owner").await;
    let expired = f.state.database.issue_invite(None).unwrap();
    f.state
        .database
        .with(|conn| {
            conn.execute("UPDATE invites SET expires_at=0", [])?;
            Ok(())
        })
        .unwrap();
    let recovery = f.state.database.issue_invite(Some("owner")).unwrap();
    for code in [expired, "0".repeat(64), recovery] {
        assert_eq!(f.call("POST","/api/v1/auth/redeem",json!({"inviteCode":code,"username":"other","displayName":"Other","password":"synthetic-password-123","device":"desktop"}),None).await.0,StatusCode::GONE);
    }
}

#[tokio::test]
async fn credentials_are_hashed_and_accounts_sessions_survive_reopen() {
    let f = Fixture::new();
    let account = f.account("player").await;
    let token = account["token"].as_str().unwrap();
    assert_eq!(token.len(), 64);
    let values: (String, String) = f
        .state
        .database
        .with(|conn| {
            Ok((
                conn.query_row("SELECT password_hash FROM accounts", [], |r| r.get(0))?,
                conn.query_row("SELECT token_hash FROM sessions", [], |r| r.get(0))?,
            ))
        })
        .unwrap();
    assert!(values.0.starts_with("$argon2id$"));
    assert!(!values.0.contains("synthetic-password-123"));
    assert_ne!(values.1, token);
    assert_eq!(values.1, auth::digest(token));
    let db = chronexus_server::db::Database::open(&f.state.config.database).unwrap();
    assert_eq!(db.sessions(token).unwrap().len(), 1);
    let (status, login) = f
        .call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"PLAYER","password":"synthetic-password-123","device":"laptop"}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(login["accountId"], account["accountId"]);
    assert_ne!(login["token"], account["token"]);
    assert_eq!(db.sessions(token).unwrap().len(), 2);
}

#[tokio::test]
async fn duplicate_username_does_not_consume_a_valid_invitation() {
    let f = Fixture::new();
    f.account("player").await;
    let code = f.state.database.issue_invite(None).unwrap();
    let payload = |name| json!({"inviteCode":code,"username":name,"displayName":"Test","password":"synthetic-password-123","device":"desktop"});
    assert_eq!(
        f.call("POST", "/api/v1/auth/redeem", payload("PLAYER"), None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        f.call("POST", "/api/v1/auth/redeem", payload("newplayer"), None)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn device_revocation_is_scoped_and_logout_revokes_bearer() {
    let f = Fixture::new();
    let a = f.account("alice").await;
    let b = f.account("bobby").await;
    let ta = a["token"].as_str().unwrap();
    let tb = b["token"].as_str().unwrap();
    let (_, devices) = f
        .call("GET", "/api/v1/auth/sessions", Value::Null, Some(ta))
        .await;
    assert_eq!(devices.as_array().unwrap().len(), 1);
    assert_eq!(devices[0]["current"], true);
    assert!(devices[0].get("token").is_none());
    let id = devices[0]["id"].as_str().unwrap();
    assert_eq!(
        f.call(
            "DELETE",
            &format!("/api/v1/auth/sessions/{id}"),
            Value::Null,
            Some(tb)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.call("GET", "/api/v1/auth/sessions", Value::Null, Some(ta))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call("POST", "/api/v1/auth/logout", Value::Null, Some(ta))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.call("GET", "/api/v1/auth/sessions", Value::Null, Some(ta))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.call("GET", "/api/v1/auth/sessions", Value::Null, Some(tb))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn revoke_one_device_preserves_others_and_revoke_all_includes_current() {
    let f = Fixture::new();
    let a = f.account("alice").await;
    let first = a["token"].as_str().unwrap();
    let (_, b) = f
        .call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"alice","password":"synthetic-password-123","device":"second"}),
            None,
        )
        .await;
    let second = b["token"].as_str().unwrap();
    let (_, devices) = f
        .call("GET", "/api/v1/auth/sessions", Value::Null, Some(first))
        .await;
    let id = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["device"] == "second")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    assert_eq!(
        f.call(
            "DELETE",
            &format!("/api/v1/auth/sessions/{id}"),
            Value::Null,
            Some(first)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.call("GET", "/api/v1/auth/sessions", Value::Null, Some(second))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.call("DELETE", "/api/v1/auth/sessions", Value::Null, Some(first))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.call("GET", "/api/v1/auth/sessions", Value::Null, Some(first))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn expired_absolute_expired_and_disabled_sessions_fail() {
    for sql in [
        "UPDATE sessions SET expires_at=0",
        "UPDATE sessions SET absolute_expires_at=0",
        "UPDATE accounts SET disabled=1",
    ] {
        let f = Fixture::new();
        let a = f.account("alice").await;
        f.state
            .database
            .with(|conn| {
                conn.execute(sql, [])?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            f.call(
                "GET",
                "/api/v1/auth/sessions",
                Value::Null,
                a["token"].as_str()
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn recovery_is_one_use_changes_password_and_revokes_all_old_sessions() {
    let f = Fixture::new();
    let a = f.account("alice").await;
    let stale = f.state.database.issue_invite(Some("alice")).unwrap();
    let code = f.state.database.issue_invite(Some("alice")).unwrap();
    let payload = |code: &str| json!({"recoveryCode":code,"password":"replacement-password-456","device":"recovered desktop"});
    assert_eq!(
        f.call("POST", "/api/v1/auth/recover", payload(&stale), None)
            .await
            .0,
        StatusCode::GONE
    );
    let (status, recovered) = f
        .call("POST", "/api/v1/auth/recover", payload(&code), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recovered["accountId"], a["accountId"]);
    assert_eq!(
        f.call(
            "GET",
            "/api/v1/auth/sessions",
            Value::Null,
            a["token"].as_str()
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.call(
            "GET",
            "/api/v1/auth/sessions",
            Value::Null,
            recovered["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call("POST", "/api/v1/auth/recover", payload(&code), None)
            .await
            .0,
        StatusCode::GONE
    );
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"alice","password":"synthetic-password-123","device":"old"}),
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"alice","password":"replacement-password-456","device":"new"}),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn login_rate_limits_persist_and_unknown_accounts_get_same_failure() {
    let f = Fixture::new();
    f.account("alice").await;
    for name in ["alice", "unknown"] {
        for _ in 0..5 {
            assert_eq!(
                f.call(
                    "POST",
                    "/api/v1/auth/login",
                    json!({"username":name,"password":"incorrect-password","device":"desktop"}),
                    None
                )
                .await
                .0,
                StatusCode::UNAUTHORIZED
            );
        }
        let restarted = AppState::new((*f.state.config).clone()).unwrap();
        assert_eq!(
            call(
                routes::router(restarted),
                "POST",
                "/api/v1/auth/login",
                json!({"username":name,"password":"incorrect-password","device":"desktop"}),
                None,
                "127.0.0.1:8080",
                None
            )
            .await
            .0,
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}

#[tokio::test]
async fn ip_limit_cannot_be_bypassed_by_new_usernames() {
    let f = Fixture::new();
    for i in 0..30 {
        f.state
            .database
            .attempt("198.51.100.1", &format!("name{i}"), "login")
            .unwrap();
    }
    assert_eq!(
        f.state
            .database
            .attempt("198.51.100.1", "other", "login")
            .unwrap_err()
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    f.state
        .database
        .attempt("198.51.100.2", "other", "login")
        .unwrap();
}

#[tokio::test]
async fn strict_json_validation_body_limits_and_safe_error_responses() {
    let f = Fixture::new();
    for body in [
        json!({"username":"alice","password":"synthetic-password-123","device":"desktop","admin":true}),
        json!({"username":"alice","password":"short","device":"desktop"}),
        json!({"username":"x' OR 1=1","password":"synthetic-password-123","device":"desktop"}),
    ] {
        let (status, error) = f.call("POST", "/api/v1/auth/login", body, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!error.to_string().contains("synthetic-password"));
    }
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"alice","password":"x".repeat(9000),"device":"desktop"}),
            None
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        f.call(
            "GET",
            "/api/v1/auth/sessions",
            Value::Null,
            Some(&"x".repeat(64))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn origin_and_host_are_checked_and_proxy_headers_are_not_trusted_in_dev() {
    let f = Fixture::new();
    assert_eq!(
        call(
            f.app.clone(),
            "GET",
            "/api/v1/meta",
            Value::Null,
            None,
            "evil.example",
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            f.app.clone(),
            "GET",
            "/api/v1/meta",
            Value::Null,
            None,
            "127.0.0.1:8080",
            Some("https://evil.example")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn production_requires_trusted_tls_proxy_and_single_client_ip() {
    let f = Fixture::new();
    let mut config = (*f.state.config).clone();
    config.loopback_http = false;
    config.public_origin = "https://sync.example.test:8443".into();
    config.authority = "sync.example.test:8443".into();
    config.trusted_proxies = vec!["192.0.2.10/32".parse().unwrap()];
    let app = routes::router(AppState::new(config).unwrap());
    for (peer, proto, forwarded, expected) in [
        ("192.0.2.11:54321", "https", "198.51.100.1", 403),
        ("192.0.2.10:54321", "http", "198.51.100.1", 403),
        (
            "192.0.2.10:54321",
            "https",
            "198.51.100.1, 198.51.100.2",
            400,
        ),
        ("192.0.2.10:54321", "https", "198.51.100.1", 200),
    ] {
        let mut req = Request::builder()
            .uri("/api/v1/meta")
            .header("host", "sync.example.test:8443")
            .header("x-forwarded-proto", proto)
            .header("x-forwarded-for", forwarded)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        let reply = app.clone().oneshot(req).await.unwrap();
        assert_eq!(reply.status().as_u16(), expected);
        assert_eq!(
            reply.headers()["strict-transport-security"],
            "max-age=31536000"
        );
    }
}

#[test]
fn configuration_fails_closed_and_supports_nonstandard_ports() {
    let base = HashMap::from([
        (
            "CHRONEXUS_PUBLIC_ORIGIN".into(),
            "https://sync.example.test:8443".into(),
        ),
        ("CHRONEXUS_TRUSTED_PROXIES".into(), "192.0.2.10".into()),
        ("CHRONEXUS_LISTEN".into(), "0.0.0.0:9876".into()),
    ]);
    let c = Config::parse(base.clone()).unwrap();
    assert_eq!(c.listen.port(), 9876);
    assert_eq!(c.authority, "sync.example.test:8443");
    for (key, value) in [
        ("CHRONEXUS_PUBLIC_ORIGIN", "http://sync.example.test"),
        (
            "CHRONEXUS_PUBLIC_ORIGIN",
            "https://user:pass@sync.example.test",
        ),
        ("CHRONEXUS_PUBLIC_ORIGIN", "https://sync.example.test/path"),
        ("CHRONEXUS_PUBLIC_ORIGIN", "https://sync.example.test?x=1"),
        ("CHRONEXUS_TRUSTED_PROXIES", ""),
        ("CHRONEXUS_TRUSTED_PROXIES", "0.0.0.0/0"),
        ("CHRONEXUS_LISTEN", "0.0.0.0:0"),
        ("CHRONEXUS_ALLOW_LOOPBACK_HTTP", "true"),
    ] {
        let mut env = base.clone();
        env.insert(key.into(), value.into());
        assert!(Config::parse(env).is_err(), "{key}");
    }
    assert!(Config::parse(HashMap::new()).is_err());
}

#[test]
fn database_rejects_future_schema_without_rewriting_and_rejects_symlinks() {
    let f = Fixture::new();
    f.state
        .database
        .with(|conn| {
            conn.pragma_update(None, "user_version", 99)?;
            Ok(())
        })
        .unwrap();
    assert!(chronexus_server::db::Database::open(&f.state.config.database).is_err());
    let version: i64 = f
        .state
        .database
        .with(|conn| Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?))
        .unwrap();
    assert_eq!(version, 99);
    #[cfg(unix)]
    {
        let link = f._directory.path().join("linked.sqlite3");
        std::os::unix::fs::symlink(&f.state.config.database, &link).unwrap();
        assert!(chronexus_server::db::Database::open(&link).is_err());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&f.state.config.database)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn blocking_workers_are_bounded_and_busy_failure_is_visible() {
    let f = Fixture::new();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(5));
    let (started, mut ready) = tokio::sync::mpsc::channel(4);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let state = f.state.clone();
        let barrier = barrier.clone();
        let started = started.clone();
        tasks.push(tokio::spawn(async move {
            state
                .blocking(move |_| {
                    started.blocking_send(()).unwrap();
                    barrier.wait();
                    Ok(())
                })
                .await
        }));
    }
    for _ in 0..4 {
        ready.recv().await.unwrap();
    }
    let error = f.state.blocking(|_| Ok(())).await.unwrap_err();
    assert_eq!(error.0, StatusCode::SERVICE_UNAVAILABLE);
    barrier.wait();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    f.state.blocking(|db| db.ready()).await.unwrap();
}

#[test]
fn foreign_unversioned_databases_are_not_migrated() {
    let directory = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    let path = directory.path().join("foreign.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("CREATE TABLE personal_data(value TEXT)", [])
        .unwrap();
    assert!(
        chronexus_server::db::Database::open(&path)
            .err()
            .unwrap()
            .contains("unrecognized")
    );
}

#[test]
fn rate_bucket_capacity_is_strict_without_locking_existing_principals() {
    let f = Fixture::new();
    f.state
        .database
        .attempt("198.51.100.1", "alice", "login")
        .unwrap();
    f.state.database.with(|conn| {
        conn.execute("WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM numbers WHERE n<9998) INSERT INTO rate_buckets SELECT printf('%064x',n),1,?1 FROM numbers",[auth::now()+900])?;
        Ok(())
    }).unwrap();
    f.state
        .database
        .attempt("198.51.100.1", "alice", "login")
        .unwrap();
    assert_eq!(
        f.state
            .database
            .attempt("198.51.100.2", "other", "login")
            .unwrap_err()
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let count: i64 = f
        .state
        .database
        .with(|conn| Ok(conn.query_row("SELECT count(*) FROM rate_buckets", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(count, 10000);
}
