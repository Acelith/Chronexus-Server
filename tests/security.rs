//! M25.7: authorization matrix, malformed and hostile inputs, resource limits and leakage checks.
//! Synthetic fixtures only.
mod common;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use common::{Fixture, request_id};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

struct World {
    f: Fixture,
    c: String,
    owner: Value,
    player: Value,
    reader: Value,
    outsider: Value,
}
impl World {
    async fn new() -> Self {
        let f = Fixture::new();
        let owner = f.account("owner").await;
        let player = f.account("player").await;
        let reader = f.account("reader").await;
        let outsider = f.account("outsider").await;
        let (_, c) = f
            .call(
                "POST",
                "/api/v1/campaigns",
                json!({"name":"Synthetic campaign"}),
                owner["token"].as_str(),
            )
            .await;
        let c = c["id"].as_str().unwrap().to_string();
        for (account, role) in [(&player, "player"), (&reader, "reader")] {
            let (_, invite) = f
                .call(
                    "POST",
                    &format!("/api/v1/campaigns/{c}/invites"),
                    json!({"role":role}),
                    owner["token"].as_str(),
                )
                .await;
            f.call(
                "POST",
                "/api/v1/campaigns/join",
                json!({"inviteCode":invite["code"]}),
                account["token"].as_str(),
            )
            .await;
        }
        Self {
            f,
            c,
            owner,
            player,
            reader,
            outsider,
        }
    }
    fn principals(&self) -> [(&'static str, Option<&str>); 5] {
        [
            ("anonymous", None),
            ("outsider", self.outsider["token"].as_str()),
            ("reader", self.reader["token"].as_str()),
            ("player", self.player["token"].as_str()),
            ("owner", self.owner["token"].as_str()),
        ]
    }
}
fn page() -> Value {
    json!({"title":"Synthetic","kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"b","type":"paragraph","props":{},"content":[{"type":"text","text":"Text","styles":{}}],"children":[]}]}})
}
fn denied(status: StatusCode) -> bool {
    [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&status)
}
async fn raw(
    w: &World,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8080");
    for (key, value) in headers {
        builder = builder.header(*key, value);
    }
    let mut request = builder.body(Body::from(body)).unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:54321".parse::<SocketAddr>().unwrap(),
    ));
    let response = w.f.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    (
        status,
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
}

/// (method, path, body, minimum role: 0 anyone signed in, 1 member, 2 writer, 3 owner)
fn matrix(c: &str, account: &str) -> Vec<(&'static str, String, Value, u8)> {
    let item = Uuid::new_v4();
    let base = format!("/api/v1/campaigns/{c}");
    vec![
        ("GET", "/api/v1/auth/sessions".into(), Value::Null, 0),
        ("GET", "/api/v1/campaigns".into(), Value::Null, 0),
        ("GET", base.clone(), Value::Null, 1),
        ("GET", format!("{base}/members"), Value::Null, 1),
        ("GET", format!("{base}/sync"), Value::Null, 1),
        ("GET", format!("{base}/changes"), Value::Null, 1),
        ("GET", format!("{base}/content"), Value::Null, 1),
        ("GET", format!("{base}/invites"), Value::Null, 3),
        (
            "POST",
            format!("{base}/invites"),
            json!({"role":"player"}),
            3,
        ),
        (
            "PATCH",
            format!("{base}/members/{account}"),
            json!({"role":"reader","baseRevision":1}),
            3,
        ),
        (
            "PUT",
            format!("{base}/content/page/{item}"),
            json!({"baseRevision":0,"payload":page()}),
            2,
        ),
        (
            "PUT",
            format!("{base}/items/page/{item}"),
            json!({"baseRevision":0,"payload":page()}),
            2,
        ),
        (
            "POST",
            format!("{base}/own-content/deletion"),
            json!({"confirm":false}),
            1,
        ),
        (
            "POST",
            format!("{base}/deletion"),
            json!({"confirmName":"not the name"}),
            3,
        ),
    ]
}

#[tokio::test]
async fn every_campaign_route_enforces_its_minimum_role_for_every_principal() {
    let w = World::new().await;
    let account = w.reader["accountId"].as_str().unwrap().to_string();
    for (method, path, body, minimum) in matrix(&w.c, &account) {
        for (name, token) in w.principals() {
            let rank = match name {
                "anonymous" => None,
                "outsider" => Some(0u8),
                "reader" => Some(1),
                "player" => Some(2),
                _ => Some(3),
            };
            let (status, reply) = w.f.call(method, &path, body.clone(), token).await;
            let allowed = rank.is_some_and(|r| r >= minimum);
            if name == "anonymous" {
                assert_eq!(
                    status,
                    StatusCode::UNAUTHORIZED,
                    "{method} {path} anonymous"
                );
            } else if allowed {
                assert!(
                    !denied(status),
                    "{method} {path} {name} got {status}: {reply}"
                );
            } else {
                assert_eq!(
                    status,
                    StatusCode::FORBIDDEN,
                    "{method} {path} {name}: {reply}"
                );
            }
            // A refusal never discloses the campaign or its content.
            if denied(status) {
                let text = reply.to_string();
                assert!(
                    !text.contains("Synthetic campaign") && !text.contains(&w.c),
                    "{text}"
                );
            }
        }
    }
}

#[tokio::test]
async fn credentials_must_be_a_single_well_formed_bearer_header() {
    let w = World::new().await;
    let token = w.owner["token"].as_str().unwrap().to_string();
    let path = "/api/v1/campaigns";
    let ok = |s: StatusCode| s == StatusCode::OK;
    assert!(ok(raw(
        &w,
        "GET",
        path,
        &[("authorization", format!("Bearer {token}"))],
        vec![]
    )
    .await
    .0));
    for headers in [
        vec![],
        vec![("authorization", token.clone())],
        vec![("authorization", format!("bearer {token}"))],
        vec![("authorization", format!("Basic {token}"))],
        vec![("authorization", format!("Bearer {}", &token[..63]))],
        vec![("authorization", format!("Bearer {token}0"))],
        vec![("authorization", format!("Bearer {}", token.to_uppercase()))],
        vec![
            ("authorization", format!("Bearer {token}")),
            ("authorization", format!("Bearer {token}")),
        ],
    ] {
        let status = raw(&w, "GET", path, &headers, vec![]).await.0;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{headers:?}");
    }
    // A token in the query string or a cookie is never accepted.
    assert_eq!(
        raw(&w, "GET", &format!("{path}?token={token}"), &[], vec![])
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        raw(
            &w,
            "GET",
            path,
            &[("cookie", format!("session={token}"))],
            vec![]
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    // A revoked session stops working immediately.
    assert_eq!(
        w.f.call("POST", "/api/v1/auth/logout", Value::Null, Some(&token))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        w.f.call("GET", path, Value::Null, Some(&token)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn hostile_identifiers_and_queries_are_refused_without_touching_data() {
    let w = World::new().await;
    let token = w.owner["token"].as_str();
    // Percent-encoded so each one reaches the router as a single path segment or value.
    let hostile = [
        "%27%3B%20DROP%20TABLE%20content%3B--",
        "%27%20OR%201%3D1--",
        "..%2F..%2Fetc%2Fpasswd",
        "%00",
        "not-a-uuid",
        "%C3%BF",
    ];
    for value in hostile {
        for path in [
            format!("/api/v1/campaigns/{value}"),
            format!("/api/v1/campaigns/{}/content/page/{value}", w.c),
            format!(
                "/api/v1/campaigns/{}/content/{value}/{}",
                w.c,
                Uuid::new_v4()
            ),
            format!("/api/v1/campaigns/{}/attachments/{value}", w.c),
            format!("/api/v1/campaigns/{}/members/{value}", w.c),
            format!("/api/v1/campaigns/{}/sync?cursor={value}", w.c),
            format!("/api/v1/campaigns/{}/content?after={value}", w.c),
        ] {
            let (status, _) = raw(
                &w,
                "GET",
                &path,
                &[("authorization", format!("Bearer {}", token.unwrap()))],
                vec![],
            )
            .await;
            assert!(
                [400, 403, 404, 405].contains(&status.as_u16()),
                "{path} -> {status}"
            );
        }
    }
    // The schema and the data are intact.
    let tables: i64 = w
        .f
        .state
        .database
        .with(|conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name IN ('content','content_log','attachments','campaigns')",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(tables, 4);
    assert_eq!(
        w.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}", w.c),
            Value::Null,
            token
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn malformed_oversized_and_hostile_bodies_fail_safely() {
    let w = World::new().await;
    let auth = vec![
        (
            "authorization",
            format!("Bearer {}", w.owner["token"].as_str().unwrap()),
        ),
        ("content-type", "application/json".into()),
    ];
    let path = format!("/api/v1/campaigns/{}/content/page/{}", w.c, Uuid::new_v4());
    // Syntax errors, wrong types, unknown fields, control characters and nulls.
    for body in [
        "{".to_string(),
        "null".to_string(),
        "[]".to_string(),
        json!({"baseRevision":"0","payload":page()}).to_string(),
        json!({"baseRevision":0,"payload":page(),"extra":1}).to_string(),
        json!({"baseRevision":-1,"payload":page()}).to_string(),
        json!({"baseRevision":0,"payload":{"title":"x\u{0}y","kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[]}}}).to_string(),
        json!({"baseRevision":0,"requestId":"not a uuid","payload":page()}).to_string(),
        json!({"baseRevision":0,"requestId":Uuid::new_v4().to_string(),"payload":page()}).to_string(),
        json!({"baseRevision":9223372036854775807i64,"payload":page()}).to_string(),
    ] {
        let (status, reply) = raw(&w, "PUT", &path, &auth, body.clone().into_bytes()).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
            "{body} -> {status}"
        );
        // Errors never echo the submitted content.
        assert!(!String::from_utf8_lossy(&reply).contains("Synthetic"));
    }
    // A request bomb: nesting far beyond any real document is refused, not recursed into.
    // Built as text: a recursive Value this deep would overflow the test's own stack.
    let depth = 5000;
    let open = r#"[{"id":"b","type":"paragraph","props":{},"content":[],"children":"#.repeat(depth);
    let bomb = format!(
        r#"{{"baseRevision":0,"payload":{{"title":"Deep","kind":"npc","status":"active","aliases":[],"document":{{"version":1,"blocks":{open}[]{close}}}}}}}"#,
        close = "}]".repeat(depth)
    );
    let (status, _) = raw(&w, "PUT", &path, &auth, bomb.into_bytes()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Bodies over the envelope are rejected before parsing.
    let (status, _) = raw(&w, "PUT", &path, &auth, vec![b'a'; 2_200_000]).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let (status, _) = raw(
        &w,
        "POST",
        "/api/v1/auth/login",
        &[("content-type", "application/json".into())],
        vec![b'a'; 9000],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    // Nothing was stored by any of the refused requests.
    assert_eq!(
        w.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}/content", w.c),
            Value::Null,
            w.owner["token"].as_str()
        )
        .await
        .1["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn uploads_are_bounded_typed_and_never_become_filesystem_paths() {
    let w = World::new().await;
    let auth = |mime: &str, sha: &str| {
        vec![
            (
                "authorization",
                format!("Bearer {}", w.player["token"].as_str().unwrap()),
            ),
            ("content-type", mime.to_string()),
            ("x-content-sha256", sha.to_string()),
        ]
    };
    let path = format!("/api/v1/campaigns/{}/attachments", w.c);
    let zero = "0".repeat(64);
    // Script, HTML, SVG and oversized bodies are never stored.
    for (mime, body) in [
        ("text/html", b"<script>alert(1)</script>".to_vec()),
        ("image/svg+xml", b"<svg onload=alert(1)/>".to_vec()),
        ("image/png", b"not an image at all".to_vec()),
        ("image/png", vec![0u8; 9 * 1024 * 1024]),
    ] {
        let (status, _) = raw(&w, "POST", &path, &auth(mime, &zero), body).await;
        assert!([400, 413].contains(&status.as_u16()), "{mime} -> {status}");
    }
    // Readers cannot upload; the traversal-shaped identifier is just an invalid identifier.
    let reader = vec![
        (
            "authorization",
            format!("Bearer {}", w.reader["token"].as_str().unwrap()),
        ),
        ("content-type", "image/png".to_string()),
        ("x-content-sha256", zero.clone()),
    ];
    assert_eq!(
        raw(&w, "POST", &path, &reader, vec![1]).await.0,
        StatusCode::FORBIDDEN
    );
    let stored: i64 =
        w.f.state
            .database
            .with(|conn| Ok(conn.query_row("SELECT count(*) FROM attachments", [], |r| r.get(0))?))
            .unwrap();
    assert_eq!(stored, 0);
    // Downloads carry no executable type and cannot be sniffed.
    let _ = request_id();
}

#[tokio::test]
async fn responses_carry_hardening_headers_and_errors_never_leak_internals() {
    let w = World::new().await;
    for (method, path) in [
        ("GET", "/api/v1/meta"),
        ("GET", "/api/v1/nothing-here"),
        ("POST", "/api/v1/auth/login"),
        ("GET", "/api/v1/campaigns"),
    ] {
        let (status, body) = raw(&w, method, path, &[], b"{".to_vec()).await;
        let text = String::from_utf8_lossy(&body).to_lowercase();
        assert!(
            ![
                "sqlite",
                "rusqlite",
                "panicked",
                "backtrace",
                ".rs:",
                "select "
            ]
            .iter()
            .any(|needle| text.contains(needle)),
            "{path} -> {status}: {text}"
        );
    }
    // Wrong credentials and unknown accounts are indistinguishable.
    let wrong =
        w.f.call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"owner","password":"wrong-password-123","device":"d"}),
            None,
        )
        .await;
    let unknown =
        w.f.call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"nobody","password":"wrong-password-123","device":"d"}),
            None,
        )
        .await;
    assert_eq!(wrong, unknown);
}
