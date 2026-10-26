mod common;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use common::{Fixture, request_id};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::HashSet, net::SocketAddr, time::Duration};
use tower::ServiceExt;
use uuid::Uuid;

struct Group {
    f: Fixture,
    c: String,
    owner: Value,
    player: Value,
}
impl Group {
    async fn new() -> Self {
        let f = Fixture::new();
        let owner = f.account("owner").await;
        let player = f.account("player").await;
        let (status, c) = f
            .call(
                "POST",
                "/api/v1/campaigns",
                json!({"name":"Synthetic campaign"}),
                owner["token"].as_str(),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        let c = c["id"].as_str().unwrap().to_string();
        let (_, invite) = f
            .call(
                "POST",
                &format!("/api/v1/campaigns/{c}/invites"),
                json!({"role":"player"}),
                owner["token"].as_str(),
            )
            .await;
        assert_eq!(
            f.call(
                "POST",
                "/api/v1/campaigns/join",
                json!({"inviteCode":invite["code"]}),
                player["token"].as_str()
            )
            .await
            .0,
            StatusCode::OK
        );
        Self {
            f,
            c,
            owner,
            player,
        }
    }
    fn path(&self, kind: &str, id: &str) -> String {
        format!("/api/v1/campaigns/{}/content/{kind}/{id}", self.c)
    }
    fn token(account: &Value) -> Option<&str> {
        account["token"].as_str()
    }
    async fn save(
        &self,
        account: &Value,
        id: &str,
        title: &str,
        visibility: &str,
        base: i64,
    ) -> (StatusCode, Value) {
        self.f
            .call(
                "PUT",
                &self.path("page", id),
                json!({"baseRevision":base,"visibility":visibility,"payload":page(title)}),
                Self::token(account),
            )
            .await
    }
    async fn sync(&self, account: &Value, cursor: &str) -> (StatusCode, Value) {
        self.f
            .call(
                "GET",
                &format!("/api/v1/campaigns/{}/sync?cursor={cursor}", self.c),
                Value::Null,
                Self::token(account),
            )
            .await
    }
}
fn page(title: &str) -> Value {
    json!({"title":title,"kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"block-1","type":"paragraph","props":{},"content":[{"type":"text","text":"Synthetic text","styles":{}}],"children":[]}]}})
}
fn id() -> String {
    Uuid::new_v4().to_string()
}
fn ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}
/// Raw request with extra headers, for the legacy Idempotency-Key and streaming tests.
async fn send(
    g: &Group,
    method: &str,
    path: &str,
    account: &Value,
    headers: &[(&str, String)],
    body: Value,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .header("content-type", "application/json")
        .header(
            "authorization",
            format!("Bearer {}", account["token"].as_str().unwrap()),
        );
    for (key, value) in headers {
        builder = builder.header(*key, value);
    }
    let mut request = builder
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:54321".parse::<SocketAddr>().unwrap(),
    ));
    g.f.app.clone().oneshot(request).await.unwrap()
}
async fn json_of(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}
async fn frame(body: &mut Body, wait: Duration) -> Option<String> {
    let next = tokio::time::timeout(wait, body.frame()).await.ok()??;
    Some(String::from_utf8(next.unwrap().into_data().unwrap().to_vec()).unwrap())
}

#[tokio::test]
async fn sync_exposes_only_authorized_state_and_content_free_tombstones() {
    let g = Group::new().await;
    let item = id();
    assert_eq!(
        g.save(&g.owner, &item, "PRIVATE-MARKER draft", "private", 0)
            .await
            .0,
        StatusCode::OK
    );
    let (_, own) = g.sync(&g.owner, "").await;
    assert_eq!(ids(&own), std::slice::from_ref(&item));
    assert_eq!(own["items"][0]["visibility"], "private");
    let (status, other) = g.sync(&g.player, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ids(&other).is_empty());
    let after_private = other["cursor"].as_str().unwrap().to_string();

    // Sharing reveals the exact projection and the author's attribution.
    assert_eq!(
        g.save(&g.owner, &item, "Shared title", "members", 1)
            .await
            .0,
        StatusCode::OK
    );
    let (_, shared) = g.sync(&g.player, &after_private).await;
    assert_eq!(ids(&shared), std::slice::from_ref(&item));
    assert_eq!(shared["items"][0]["deleted"], false);
    assert_eq!(shared["items"][0]["payload"]["title"], "Shared title");
    let after_share = shared["cursor"].as_str().unwrap().to_string();

    // Withdrawal reaches the member as a payload-free tombstone while the owner keeps the draft.
    assert_eq!(
        g.save(&g.owner, &item, "PRIVATE-MARKER again", "private", 2)
            .await
            .0,
        StatusCode::OK
    );
    let (_, gone) = g.sync(&g.player, &after_share).await;
    assert_eq!(gone["items"][0]["deleted"], true);
    assert!(gone["items"][0]["payload"].is_null());
    assert!(!gone.to_string().contains("PRIVATE-MARKER"));
    let after_gone = gone["cursor"].as_str().unwrap().to_string();
    let (_, kept) = g.sync(&g.owner, &after_share).await;
    assert_eq!(kept["items"][0]["payload"]["title"], "PRIVATE-MARKER again");

    // Deleting a private item is invisible to the member and a tombstone for the author.
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!("{}?baseRevision=3", g.path("page", &item)),
            Value::Null,
            Group::token(&g.owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, quiet) = g.sync(&g.player, &after_gone).await;
    assert!(ids(&quiet).is_empty());
    let (_, tomb) = g.sync(&g.owner, &after_gone).await;
    assert_eq!(tomb["items"][0]["deleted"], true);
    assert!(tomb["items"][0]["payload"].is_null());
}

#[tokio::test]
async fn sync_pages_are_bounded_resumable_and_deduplicated() {
    let g = Group::new().await;
    let mut created = HashSet::new();
    for n in 0..505 {
        let item = id();
        assert_eq!(
            g.save(&g.owner, &item, &format!("Page {n}"), "private", 0)
                .await
                .0,
            StatusCode::OK
        );
        created.insert(item);
    }
    // Re-saving an item produces another log row but one delivered item per page.
    let first_item = created.iter().next().unwrap().clone();
    assert_eq!(
        g.save(&g.owner, &first_item, "Edited", "private", 1)
            .await
            .0,
        StatusCode::OK
    );
    let (_, one) = g.sync(&g.owner, "").await;
    assert_eq!(one["more"], true);
    assert!(ids(&one).len() <= 500);
    let (_, two) = g.sync(&g.owner, one["cursor"].as_str().unwrap()).await;
    assert_eq!(two["more"], false);
    let mut seen: HashSet<String> = ids(&one).into_iter().collect();
    seen.extend(ids(&two));
    assert_eq!(seen, created);
    // The final cursor is the head: nothing new, nothing rescanned.
    let (_, idle) = g.sync(&g.owner, two["cursor"].as_str().unwrap()).await;
    assert!(ids(&idle).is_empty());
    assert_eq!(idle["cursor"], two["cursor"]);
}

#[tokio::test]
async fn compacted_or_future_cursors_fail_closed_and_listing_resumes_from_head() {
    let g = Group::new().await;
    for n in 0..6 {
        g.save(&g.owner, &id(), &format!("Page {n}"), "private", 0)
            .await;
    }
    let (_, early) = g.sync(&g.owner, "").await;
    let early_cursor = early["cursor"].as_str().unwrap().to_string();
    for n in 0..6 {
        g.save(&g.owner, &id(), &format!("More {n}"), "private", 0)
            .await;
    }
    assert!(g.f.state.database.compact_log(&g.c, 3).unwrap() > 0);
    for stale in ["", "0", early_cursor.as_str(), "99999"] {
        assert_eq!(g.sync(&g.owner, stale).await.0, StatusCode::GONE, "{stale}");
    }
    assert_eq!(g.sync(&g.owner, "-1").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(g.sync(&g.owner, "abc").await.0, StatusCode::BAD_REQUEST);
    let (_, list) =
        g.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}/content", g.c),
            Value::Null,
            Group::token(&g.owner),
        )
        .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 12);
    let head = list["head"].as_str().unwrap();
    let (status, resumed) = g.sync(&g.owner, head).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ids(&resumed).is_empty());
    // New changes after the resync arrive normally.
    g.save(&g.owner, &id(), "After resync", "private", 0).await;
    assert_eq!(ids(&g.sync(&g.owner, head).await.1).len(), 1);
}

#[tokio::test]
async fn keyed_delete_and_unshare_are_acknowledged_once_and_bound_to_their_request() {
    let g = Group::new().await;
    let item = id();
    g.save(&g.owner, &item, "Keyed", "members", 0).await;
    let path = g.path("page", &item);
    let unshare = request_id();
    let call = |op: &str, url: String| {
        let token = Group::token(&g.owner).map(String::from);
        let f = &g.f;
        let op = op.to_string();
        async move { f.call(&op, &url, Value::Null, token.as_deref()).await }
    };
    let url = format!("{path}/sharing?baseRevision=1&requestId={unshare}");
    let first = call("DELETE", url.clone()).await;
    assert_eq!(first, (StatusCode::OK, json!({"revision":2})));
    // A lost response is retried safely; without the key the same call is 404.
    assert_eq!(call("DELETE", url).await, first);
    assert_eq!(
        call("DELETE", format!("{path}/sharing?baseRevision=1"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    // The same ID cannot be reused for another operation or revision.
    assert_eq!(
        call(
            "DELETE",
            format!("{path}?baseRevision=2&requestId={unshare}")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let delete = request_id();
    let url = format!("{path}?baseRevision=2&requestId={delete}");
    let deleted = call("DELETE", url.clone()).await;
    assert_eq!(deleted, (StatusCode::OK, json!({"revision":3})));
    assert_eq!(call("DELETE", url).await, deleted);
    assert_eq!(
        call("GET", path).await.0,
        StatusCode::NOT_FOUND,
        "a receipt never resurrects content"
    );
}

#[tokio::test]
async fn legacy_writes_accept_an_idempotency_key_header() {
    let g = Group::new().await;
    let item = id();
    let key = request_id();
    let path = format!("/api/v1/campaigns/{}/items/page/{item}", g.c);
    let (group, path_ref, key_ref) = (&g, &path, &key);
    let put = |base: i64| async move {
        send(
            group,
            "PUT",
            path_ref,
            &group.owner,
            &[("idempotency-key", key_ref.clone())],
            json!({"baseRevision":base,"payload":page("Legacy")}),
        )
        .await
    };
    let first = json_of(put(0).await).await;
    assert_eq!(first, (StatusCode::OK, json!({"revision":1})));
    assert_eq!(json_of(put(0).await).await, first);
    // Same key, different revision: refused rather than treated as a new save.
    assert_eq!(json_of(put(1).await).await.0, StatusCode::CONFLICT);
    let delete_key = request_id();
    let delete_ref = &delete_key;
    let delete = || async move {
        send(
            group,
            "DELETE",
            &format!("{path_ref}?baseRevision=1"),
            &group.owner,
            &[("idempotency-key", delete_ref.clone())],
            Value::Null,
        )
        .await
    };
    let removed = json_of(delete().await).await;
    assert_eq!(removed, (StatusCode::OK, json!({"revision":2})));
    assert_eq!(json_of(delete().await).await, removed);
    let both = send(
        &g,
        "DELETE",
        &format!("{path}?baseRevision=2&requestId={}", request_id()),
        &g.owner,
        &[("idempotency-key", request_id())],
        Value::Null,
    )
    .await;
    assert_eq!(both.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn request_ids_must_be_recent_uuid_v7_and_old_receipts_are_compacted() {
    let g = Group::new().await;
    let path = g.path("page", &id());
    let call = |request: String| {
        g.f.call(
            "PUT",
            &path,
            json!({"requestId":request,"baseRevision":0,"payload":page("Window")}),
            Group::token(&g.owner),
        )
    };
    assert_eq!(
        call(Uuid::new_v4().to_string()).await.0,
        StatusCode::BAD_REQUEST
    );
    let at = |seconds: i64| {
        Uuid::new_v7(uuid::Timestamp::from_unix(
            uuid::NoContext,
            seconds as u64,
            0,
        ))
        .to_string()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // A retry older than the window is refused, so an expired receipt can never matter.
    assert_eq!(call(at(now - 31 * 86_400)).await.0, StatusCode::GONE);
    assert_eq!(call(at(now + 3_600)).await.0, StatusCode::BAD_REQUEST);
    g.f.state
        .database
        .with(|conn| {
            conn.execute(
                "INSERT INTO save_receipts VALUES(?1,?2,?3,'page','x',?4,1,'save',1)",
                rusqlite::params![
                    g.c,
                    g.owner["accountId"].as_str().unwrap(),
                    at(now - 40 * 86_400),
                    "0".repeat(64)
                ],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(call(at(now - 3_600)).await.0, StatusCode::OK);
    let remaining: i64 = g
        .f
        .state
        .database
        .with(|conn| Ok(conn.query_row("SELECT count(*) FROM save_receipts", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn notifications_are_content_free_member_scoped_and_bounded() {
    let g = Group::new().await;
    let events = format!("/api/v1/campaigns/{}/events", g.c);
    let (group, events_ref) = (&g, &events);
    let open = |account: &Value| {
        let account = account.clone();
        async move { send(group, "GET", events_ref, &account, &[], Value::Null).await }
    };
    let response = open(&g.player).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let mut player = response.into_body();
    let mut owner = open(&g.owner).await.into_body();
    let wait = Duration::from_millis(400);
    assert_eq!(
        frame(&mut player, wait).await.unwrap(),
        "event: ready\ndata: {}\n\n"
    );
    assert!(frame(&mut owner, wait).await.unwrap().contains("ready"));

    // A private edit notifies only its author.
    let item = id();
    g.save(&g.owner, &item, "PRIVATE-MARKER", "private", 0)
        .await;
    assert_eq!(
        frame(&mut owner, wait).await.unwrap(),
        "event: changed\ndata: {}\n\n"
    );
    assert!(
        frame(&mut player, Duration::from_millis(250))
            .await
            .is_none()
    );
    // Sharing notifies every member with no identifiers or cursors in the frame.
    g.save(&g.owner, &item, "Shared", "members", 1).await;
    let hint = frame(&mut player, wait).await.unwrap();
    assert_eq!(hint, "event: changed\ndata: {}\n\n");
    assert!(!hint.contains(&item));

    // Non-members and anonymous clients cannot subscribe.
    let outsider = g.f.account("outsider").await;
    assert_ne!(open(&outsider).await.status(), StatusCode::OK);
    let mut anonymous = Request::builder()
        .uri(&events)
        .header("host", "127.0.0.1:8080")
        .body(Body::empty())
        .unwrap();
    anonymous.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:54321".parse::<SocketAddr>().unwrap(),
    ));
    assert_eq!(
        g.f.app.clone().oneshot(anonymous).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    // At most three streams per account; a closed stream frees its slot.
    let second = open(&g.player).await;
    let third = open(&g.player).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(third.status(), StatusCode::OK);
    assert_eq!(
        open(&g.player).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    drop((player, second));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(open(&g.player).await.status(), StatusCode::OK);
}
