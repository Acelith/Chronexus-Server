mod common;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use common::{Fixture, request_id};
use http_body_util::BodyExt;
use image::{DynamicImage, ImageFormat};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Cursor, net::SocketAddr};
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
        let g = Self {
            f,
            c,
            owner,
            player,
        };
        g.join(&g.player.clone()).await;
        g
    }
    async fn join(&self, account: &Value) {
        let (_, invite) = self
            .f
            .call(
                "POST",
                &format!("/api/v1/campaigns/{}/invites", self.c),
                json!({"role":"player"}),
                self.owner["token"].as_str(),
            )
            .await;
        assert_eq!(
            self.f
                .call(
                    "POST",
                    "/api/v1/campaigns/join",
                    json!({"inviteCode":invite["code"]}),
                    account["token"].as_str()
                )
                .await
                .0,
            StatusCode::OK
        );
    }
    fn token(account: &Value) -> Option<&str> {
        account["token"].as_str()
    }
    fn path(&self, kind: &str, id: &str) -> String {
        format!("/api/v1/campaigns/{}/content/{kind}/{id}", self.c)
    }
    async fn save(
        &self,
        account: &Value,
        id: &str,
        title: &str,
        visibility: &str,
        base: i64,
        key: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut body = json!({"baseRevision":base,"visibility":visibility,"payload":page(title)});
        if let Some(key) = key {
            body["requestId"] = json!(key);
        }
        self.f
            .call("PUT", &self.path("page", id), body, Self::token(account))
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
    async fn delete_campaign(&self, account: &Value, name: &str) -> (StatusCode, Value) {
        self.f
            .call(
                "POST",
                &format!("/api/v1/campaigns/{}/deletion", self.c),
                json!({"confirmName":name}),
                Self::token(account),
            )
            .await
    }
    async fn delete_account(
        &self,
        account: &Value,
        password: &str,
        owned: bool,
    ) -> (StatusCode, Value) {
        self.f
            .call(
                "POST",
                "/api/v1/auth/account/deletion",
                json!({"password":password,"deleteOwnedCampaigns":owned}),
                Self::token(account),
            )
            .await
    }
    fn count(&self, table: &str) -> i64 {
        self.f
            .state
            .database
            .with(|conn| {
                Ok(conn.query_row(
                    &format!(
                        "SELECT count(*) FROM {table} WHERE campaign_id='{}'",
                        self.c
                    ),
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }
}
fn page(title: &str) -> Value {
    json!({"title":title,"kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"block-1","type":"paragraph","props":{},"content":[{"type":"text","text":"Synthetic text","styles":{}}],"children":[]}]}})
}
fn id() -> String {
    Uuid::new_v4().to_string()
}
async fn upload(g: &Group, account: &Value) -> Value {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 2)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    let bytes = out.into_inner();
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/campaigns/{}/attachments", g.c))
        .header("host", "127.0.0.1:8080")
        .header("content-type", "image/png")
        .header("x-content-sha256", format!("{:x}", Sha256::digest(&bytes)))
        .header(
            "authorization",
            format!("Bearer {}", account["token"].as_str().unwrap()),
        )
        .body(Body::from(bytes))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:54321".parse::<SocketAddr>().unwrap(),
    ));
    let response = g.f.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn deleting_a_campaign_needs_the_owner_and_the_exact_name_and_removes_every_payload() {
    let g = Group::new().await;
    let (shared, private) = (id(), id());
    g.save(&g.owner, &shared, "Shared page", "members", 0, None)
        .await;
    g.save(
        &g.owner,
        &private,
        "PRIVATE-MARKER",
        "private",
        0,
        Some(&request_id()),
    )
    .await;
    let attachment = upload(&g, &g.owner).await;
    assert!(g.count("content") > 0 && g.count("attachments") == 1);
    // Not the owner, or the wrong name: nothing happens.
    assert_eq!(
        g.delete_campaign(&g.player, "Synthetic campaign").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        g.delete_campaign(&g.owner, "synthetic campaign").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(g.sync(&g.player, "").await.0, StatusCode::OK);

    assert_eq!(
        g.delete_campaign(&g.owner, "Synthetic campaign").await.0,
        StatusCode::NO_CONTENT
    );
    for table in [
        "content",
        "attachments",
        "content_log",
        "save_receipts",
        "campaign_invites",
        "published_items",
        "content_dependencies",
        "content_attachments",
    ] {
        assert_eq!(g.count(table), 0, "{table}");
    }
    // Former members are told, everyone else just sees a refusal.
    for account in [&g.owner, &g.player] {
        let (status, body) = g.sync(account, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "This campaign was deleted.");
    }
    let outsider = g.f.account("outsider").await;
    assert_eq!(g.sync(&outsider, "").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        g.f.call(
            "GET",
            &format!(
                "/api/v1/campaigns/{}/attachments/{}",
                g.c,
                attachment["id"].as_str().unwrap()
            ),
            Value::Null,
            Group::token(&g.owner)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, listed) =
        g.f.call(
            "GET",
            "/api/v1/campaigns",
            Value::Null,
            Group::token(&g.owner),
        )
        .await;
    assert!(listed.as_array().unwrap().is_empty());
    // The stored database holds no trace of the private marker.
    let dump: String =
        g.f.state
            .database
            .with(|conn| {
                let mut out = String::new();
                for table in ["content", "content_log", "attachments", "save_receipts"] {
                    let mut q = conn.prepare(&format!("SELECT * FROM {table}"))?;
                    let n = q.column_count();
                    let mut rows = q.query([])?;
                    while let Some(row) = rows.next()? {
                        for i in 0..n {
                            out.push_str(&format!("{:?}", row.get_ref(i)?));
                        }
                    }
                }
                Ok(out)
            })
            .unwrap();
    assert!(!dump.contains("PRIVATE-MARKER"));
}

#[tokio::test]
async fn erasing_your_own_server_copy_keeps_others_content_membership_and_idempotent_retries() {
    let g = Group::new().await;
    let (theirs, mine_shared, mine_private) = (id(), id(), id());
    g.save(&g.owner, &theirs, "Owner page", "members", 0, None)
        .await;
    let old_key = request_id();
    g.save(
        &g.player,
        &mine_shared,
        "Player claim",
        "members",
        0,
        Some(&old_key),
    )
    .await;
    g.save(
        &g.player,
        &mine_private,
        "Player secret",
        "private",
        0,
        None,
    )
    .await;
    let attachment = upload(&g, &g.player).await;
    let (_, before) = g.sync(&g.owner, "").await;
    let cursor = before["cursor"].as_str().unwrap().to_string();

    let path = format!("/api/v1/campaigns/{}/own-content/deletion", g.c);
    assert_eq!(
        g.f.call(
            "POST",
            &path,
            json!({"confirm":false}),
            Group::token(&g.player)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, erased) =
        g.f.call(
            "POST",
            &path,
            json!({"confirm":true}),
            Group::token(&g.player),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(erased, json!({"items":2,"attachments":1}));

    // The owner's copy of the player's shared claim becomes a tombstone; their own page stays.
    let (_, after) = g.sync(&g.owner, &cursor).await;
    let gone: Vec<&Value> = after["items"].as_array().unwrap().iter().collect();
    assert_eq!(gone.len(), 1);
    assert_eq!(gone[0]["id"], mine_shared);
    assert_eq!(gone[0]["deleted"], true);
    assert!(gone[0]["payload"].is_null());
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("page", &theirs),
            Value::Null,
            Group::token(&g.owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    // The player is still a member and the upload is gone.
    assert_eq!(g.sync(&g.player, "").await.0, StatusCode::OK);
    assert_eq!(
        g.f.call(
            "GET",
            &format!(
                "/api/v1/campaigns/{}/attachments/{}",
                g.c,
                attachment["id"].as_str().unwrap()
            ),
            Value::Null,
            Group::token(&g.player)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // A lost-response retry of the old save is acknowledged but never brings the item back.
    let (status, ack) = g
        .save(
            &g.player,
            &mine_shared,
            "Player claim",
            "members",
            0,
            Some(&old_key),
        )
        .await;
    assert_eq!(
        (status, ack["revision"].as_i64()),
        (StatusCode::OK, Some(1))
    );
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("page", &mine_shared),
            Value::Null,
            Group::token(&g.player)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn deleting_an_account_checks_the_password_protects_shared_campaigns_and_erases_everywhere() {
    let g = Group::new().await;
    let claim = id();
    g.save(
        &g.player,
        &claim,
        "PRIVATE-MARKER claim",
        "members",
        0,
        None,
    )
    .await;
    let (_, before) = g.sync(&g.owner, "").await;
    let cursor = before["cursor"].as_str().unwrap().to_string();
    assert_eq!(
        g.delete_account(&g.player, "not the password at all", false)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // The owner of a campaign other members use must decide explicitly.
    assert_eq!(
        g.delete_account(&g.owner, "synthetic-password-123", false)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        g.sync(&g.owner, "").await.0,
        StatusCode::OK,
        "nothing changed"
    );

    assert_eq!(
        g.delete_account(&g.player, "synthetic-password-123", false)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    // The session is dead, the account cannot sign in, and the claim is gone for the owner.
    assert_eq!(g.sync(&g.player, "").await.0, StatusCode::UNAUTHORIZED);
    let (status, _) =
        g.f.call(
            "POST",
            "/api/v1/auth/login",
            json!({"username":"player","password":"synthetic-password-123","device":"again"}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, after) = g.sync(&g.owner, &cursor).await;
    assert_eq!(after["items"][0]["deleted"], true);
    assert!(!after.to_string().contains("PRIVATE-MARKER"));
    let (_, members) =
        g.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}/members", g.c),
            Value::Null,
            Group::token(&g.owner),
        )
        .await;
    assert_eq!(members.as_array().unwrap().len(), 1);
    // The username is free again, and the stub carries no personal data.
    let stub: (String, String) =
        g.f.state
            .database
            .with(|conn| {
                Ok(conn.query_row(
                    "SELECT username,display_name FROM accounts WHERE deleted_at IS NOT NULL",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
    assert!(stub.0.starts_with("deleted-") && stub.1 == "Deleted account");

    // With the explicit flag the owner's campaign goes with the account.
    assert_eq!(
        g.delete_account(&g.owner, "synthetic-password-123", true)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(g.count("content"), 0);
}

#[tokio::test]
async fn a_removed_member_cannot_come_back_with_old_writes_and_nothing_is_republished() {
    let g = Group::new().await;
    let item = id();
    g.save(&g.player, &item, "Player claim", "members", 0, None)
        .await;
    let (_, members) =
        g.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}/members", g.c),
            Value::Null,
            Group::token(&g.owner),
        )
        .await;
    let revision = members
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "player")
        .unwrap()["revision"]
        .as_i64()
        .unwrap();
    let account = g.player["accountId"].as_str().unwrap();
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!(
                "/api/v1/campaigns/{}/members/{account}?baseRevision={revision}",
                g.c
            ),
            Value::Null,
            Group::token(&g.owner)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    // Revocation is immediate: the old session can neither read nor write.
    assert_eq!(g.sync(&g.player, "").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        g.save(&g.player, &item, "Offline edit", "members", 1, None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (_, feed) = g.sync(&g.owner, "").await;
    assert_eq!(
        feed["items"][0]["deleted"], true,
        "the claim was withdrawn for everyone else"
    );

    // Rejoining by a new invitation does not republish the old claim; a stale write conflicts.
    g.join(&g.player.clone()).await;
    assert_eq!(g.sync(&g.owner, "").await.1["items"][0]["deleted"], true);
    let (status, current) = g
        .save(&g.player, &item, "Offline edit", "members", 1, None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(current["revision"].as_i64().unwrap() > 1);
}

#[tokio::test]
async fn purged_tombstones_force_a_relisting_and_never_let_a_stale_client_resurrect_items() {
    let g = Group::new().await;
    let item = id();
    g.save(&g.owner, &item, "Short lived", "members", 0, None)
        .await;
    let (_, first) = g.sync(&g.player, "").await;
    let cursor = first["cursor"].as_str().unwrap().to_string();
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!("{}?baseRevision=1", g.path("page", &item)),
            Value::Null,
            Group::token(&g.owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    // Recent tombstones stay; old ones go.
    assert_eq!(g.f.state.database.purge_tombstones(30).unwrap(), 0);
    g.f.state
        .database
        .with(|conn| {
            conn.execute("UPDATE content SET updated_at=1 WHERE deleted=1", [])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(g.f.state.database.purge_tombstones(30).unwrap(), 1);
    assert_eq!(g.count("content"), 0);
    // The stale device cannot learn the deletion from the log any more, so it must list again.
    assert_eq!(g.sync(&g.player, &cursor).await.0, StatusCode::GONE);
    let (_, list) =
        g.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}/content", g.c),
            Value::Null,
            Group::token(&g.player),
        )
        .await;
    assert!(list["items"].as_array().unwrap().is_empty());
    assert_eq!(
        g.sync(&g.player, list["head"].as_str().unwrap()).await.0,
        StatusCode::OK
    );
    // An old offline write against the purged item is refused, not silently re-created.
    let (status, _) = g.save(&g.owner, &item, "Zombie", "members", 2, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_restored_backup_keeps_revocations_and_operators_can_end_every_old_session() {
    let g = Group::new().await;
    let (_, members) =
        g.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}/members", g.c),
            Value::Null,
            Group::token(&g.owner),
        )
        .await;
    let revision = members
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "player")
        .unwrap()["revision"]
        .as_i64()
        .unwrap();
    // The backup is taken while the player is still a member and still signed in...
    let copy = g.f._directory.path().join("backup.sqlite3");
    g.f.state
        .database
        .with(|conn| {
            conn.execute("VACUUM INTO ?1", [copy.to_str().unwrap()])?;
            Ok(())
        })
        .unwrap();
    // ...then the player is removed.
    let account = g.player["accountId"].as_str().unwrap();
    g.f.call(
        "DELETE",
        &format!(
            "/api/v1/campaigns/{}/members/{account}?baseRevision={revision}",
            g.c
        ),
        Value::Null,
        Group::token(&g.owner),
    )
    .await;
    // Restoring the older backup would bring the membership and the player's session back...
    let restored = chronexus_server::db::Database::open(&copy).unwrap();
    let player_token = g.player["token"].as_str().unwrap();
    assert!(
        restored
            .authorize_content(player_token, &g.c, false)
            .is_ok()
    );
    // ...which is why the operator ends every session after a restore.
    assert_eq!(restored.revoke_all_sessions().unwrap(), 2);
    assert!(
        restored
            .authorize_content(player_token, &g.c, false)
            .is_err()
    );
    assert!(
        restored
            .authorize_content(g.owner["token"].as_str().unwrap(), &g.c, false)
            .is_err()
    );
    // The live server already refuses the removed member.
    assert_eq!(g.sync(&g.player, "").await.0, StatusCode::FORBIDDEN);
}
