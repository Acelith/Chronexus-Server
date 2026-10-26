mod common;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use common::Fixture;
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
    reader: Value,
}
impl Group {
    async fn new() -> Self {
        let f = Fixture::new();
        let owner = f.account("owner").await;
        let player = f.account("player").await;
        let reader = f.account("reader").await;
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
        for (account, role) in [(&player, "player"), (&reader, "reader")] {
            let (_, invite) = f
                .call(
                    "POST",
                    &format!("/api/v1/campaigns/{c}/invites"),
                    json!({"role":role}),
                    owner["token"].as_str(),
                )
                .await;
            assert_eq!(
                f.call(
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
        Self {
            f,
            c,
            owner,
            player,
            reader,
        }
    }
    fn path(&self, kind: &str, id: &str) -> String {
        format!("/api/v1/campaigns/{}/content/{kind}/{id}", self.c)
    }
    async fn save(
        &self,
        account: &Value,
        kind: &str,
        id: &str,
        payload: Value,
        visibility: Option<&str>,
        base: i64,
    ) -> (StatusCode, Value) {
        let mut body = json!({"baseRevision":base,"payload":payload});
        if let Some(visibility) = visibility {
            body["visibility"] = json!(visibility);
        }
        self.f
            .call("PUT", &self.path(kind, id), body, account["token"].as_str())
            .await
    }
    async fn feed(&self, account: &Value, since: &str) -> (StatusCode, Value) {
        self.f
            .call(
                "GET",
                &format!("/api/v1/campaigns/{}/changes?since={since}", self.c),
                Value::Null,
                account["token"].as_str(),
            )
            .await
    }
}
fn page(title: &str) -> Value {
    json!({"title":title,"kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"block-1","type":"paragraph","props":{},"content":[{"type":"text","text":"Synthetic public text","styles":{}}],"children":[]}]}})
}
fn relationship(a: &str, b: &str) -> Value {
    json!({"fromPageId":a,"toPageId":b,"label":"Knows","certainty":"suspected","evidence":"A player claim","notes":""})
}
fn image_bytes(format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 2)
        .write_to(&mut out, format)
        .unwrap();
    out.into_inner()
}
async fn raw(
    f: &Fixture,
    method: &str,
    path: &str,
    token: Option<&str>,
    mime: &str,
    checksum: &str,
    data: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .header("content-type", mime)
        .header("x-content-sha256", checksum);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let mut req = builder.body(Body::from(data)).unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:54321".parse::<SocketAddr>().unwrap(),
    ));
    let response = f.app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
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
async fn upload(g: &Group, account: &Value, bytes: &[u8], mime: &str) -> Value {
    let (status, body) = raw(
        &g.f,
        "POST",
        &format!("/api/v1/campaigns/{}/attachments", g.c),
        account["token"].as_str(),
        mime,
        &format!("{:x}", Sha256::digest(bytes)),
        bytes.to_vec(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn private_by_default_including_against_campaign_owner_and_explicit_publication() {
    let g = Group::new().await;
    let id = Uuid::new_v4().to_string();
    let private = page("PRIVATE-MARKER");
    assert_eq!(
        g.save(&g.player, "page", &id, private.clone(), None, 0)
            .await
            .0,
        StatusCode::OK
    );
    for account in [&g.owner, &g.reader] {
        assert_eq!(
            g.f.call(
                "GET",
                &g.path("page", &id),
                Value::Null,
                account["token"].as_str()
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        assert!(
            !g.feed(account, "")
                .await
                .1
                .to_string()
                .contains("PRIVATE-MARKER")
        );
        let (_, list) =
            g.f.call(
                "GET",
                &format!("/api/v1/campaigns/{}/content", g.c),
                Value::Null,
                account["token"].as_str(),
            )
            .await;
        assert_eq!(list["items"], json!([]));
    }
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("page", &id),
            Value::Null,
            g.player["token"].as_str()
        )
        .await
        .1["payload"],
        private
    );
    assert_eq!(
        g.save(
            &g.player,
            "page",
            &id,
            page("Explicitly selected public title"),
            Some("members"),
            1
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, feed) = g.feed(&g.reader, "").await;
    assert_eq!(feed["items"][0]["authorId"], g.player["accountId"]);
    assert_eq!(
        feed["items"][0]["payload"]["title"],
        "Explicitly selected public title"
    );
}
#[tokio::test]
async fn roles_authorship_and_cross_campaign_boundaries_apply_to_reads_writes_conflicts() {
    let g = Group::new().await;
    let id = Uuid::new_v4().to_string();
    assert_eq!(
        g.save(&g.player, "page", &id, page("Public"), Some("members"), 0)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        g.save(&g.reader, "page", &id, page("Forged"), Some("members"), 1)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        g.save(
            &g.owner,
            "page",
            &id,
            page("Owner cannot rewrite author"),
            Some("members"),
            1
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let secret = Uuid::new_v4().to_string();
    g.save(&g.player, "page", &secret, page("PRIVATE-MARKER"), None, 0)
        .await;
    let (status, body) = g
        .save(
            &g.owner,
            "page",
            &secret,
            page("attempt"),
            Some("members"),
            0,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.to_string().contains("PRIVATE-MARKER"));
    let outsider = g.f.account("outsider").await;
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("page", &id),
            Value::Null,
            outsider["token"].as_str()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(g.feed(&outsider, "").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!("{}?baseRevision=1", g.path("page", &id)),
            Value::Null,
            g.owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn shared_relationships_require_explicit_endpoints_and_revocation_withdraws_dependents() {
    let g = Group::new().await;
    let a = Uuid::new_v4().to_string();
    let b = Uuid::new_v4().to_string();
    let rel = Uuid::new_v4().to_string();
    g.save(&g.owner, "page", &a, page("First"), Some("members"), 0)
        .await;
    g.save(&g.player, "page", &b, page("Private second"), None, 0)
        .await;
    assert_eq!(
        g.save(
            &g.player,
            "relationship",
            &rel,
            relationship(&a, &b),
            Some("members"),
            0
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        g.feed(&g.reader, "").await.1["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    g.save(&g.player, "page", &b, page("Second"), Some("members"), 1)
        .await;
    assert_eq!(
        g.save(
            &g.player,
            "relationship",
            &rel,
            relationship(&a, &b),
            Some("members"),
            0
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, before) = g.feed(&g.reader, "").await;
    let cursor = before["cursor"].as_str().unwrap();
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!("{}/sharing?baseRevision=1", g.path("page", &a)),
            Value::Null,
            g.owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, after) = g.feed(&g.reader, cursor).await;
    assert_eq!(after["items"].as_array().unwrap().len(), 2);
    assert!(
        after["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["deleted"] == true && v["payload"].is_null())
    );
    let (_, own) =
        g.f.call(
            "GET",
            &g.path("relationship", &rel),
            Value::Null,
            g.player["token"].as_str(),
        )
        .await;
    assert_eq!(own["visibility"], "private");
    assert_eq!(own["payload"]["certainty"], "suspected");
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("relationship", &rel),
            Value::Null,
            g.reader["token"].as_str()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn selected_sheet_section_and_image_extensions_do_not_break_legacy_feed() {
    let g = Group::new().await;
    let id = Uuid::new_v4().to_string();
    let char_id = Uuid::new_v4().to_string();
    let section = json!({"characterId":char_id,"name":"Selected combat fields","section":"combat","fields":[{"key":"armorClass","label":"Armor class","value":"0"},{"key":"inspiration","label":"Inspiration","value":"No"}]});
    assert_eq!(
        g.save(
            &g.player,
            "sheetSection",
            &id,
            section.clone(),
            Some("members"),
            0
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(g.feed(&g.reader, "").await.1["items"], json!([]));
    let (_, item) =
        g.f.call(
            "GET",
            &g.path("sheetSection", &id),
            Value::Null,
            g.reader["token"].as_str(),
        )
        .await;
    assert_eq!(item["payload"], section);
    assert!(!item.to_string().contains("fullSheet"));
    let mut malicious = section;
    malicious["fullSheet"] = json!({"privateNotes":"PRIVATE-MARKER"});
    assert_eq!(
        g.save(
            &g.player,
            "sheetSection",
            &id,
            malicious,
            Some("members"),
            1
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn unknown_nested_metadata_active_links_and_oversize_content_are_rejected() {
    let g = Group::new().await;
    let id = Uuid::new_v4().to_string();
    let mut hidden = page("Hidden");
    hidden["document"]["blocks"][0]["props"]["privateNotes"] = json!("PRIVATE-MARKER");
    assert_eq!(
        g.save(&g.owner, "page", &id, hidden, Some("members"), 0)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut link = page("Link");
    link["document"]["blocks"][0]["content"] =
        json!([{"type":"link","href":"javascript:alert(1)","content":[]}]);
    assert_eq!(
        g.save(&g.owner, "page", &id, link, Some("members"), 0)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut huge = page("Huge");
    huge["document"]["blocks"][0]["content"][0]["text"] = json!("a".repeat(2_000_001));
    assert_eq!(
        g.save(&g.owner, "page", &id, huge, Some("members"), 0)
            .await
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut duplicate = page("Repeated block");
    let block = duplicate["document"]["blocks"][0].clone();
    duplicate["document"]["blocks"]
        .as_array_mut()
        .unwrap()
        .push(block);
    assert_eq!(
        g.save(&g.owner, "page", &id, duplicate, Some("members"), 0)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(g.feed(&g.reader, "").await.1["items"], json!([]));
}
#[tokio::test]
async fn mentions_preserve_label_without_uploading_or_asserting_private_target() {
    let g = Group::new().await;
    let target = Uuid::new_v4().to_string();
    let id = Uuid::new_v4().to_string();
    let mut content = page("Public mention");
    content["document"]["blocks"][0]["content"] = json!([{"type":"pageMention","props":{"pageId":target,"label":"A name already in shared text"}}]);
    assert_eq!(
        g.save(&g.player, "page", &id, content.clone(), Some("members"), 0)
            .await
            .0,
        StatusCode::OK
    );
    let (_, feed) = g.feed(&g.reader, "").await;
    assert_eq!(feed["items"].as_array().unwrap().len(), 1);
    assert_eq!(feed["items"][0]["payload"], content);
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("page", &target),
            Value::Null,
            g.reader["token"].as_str()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn conflicts_and_legacy_unshare_preserve_private_copy_and_monotonic_revisions() {
    let g = Group::new().await;
    let id = Uuid::new_v4().to_string();
    let endpoint = format!("/api/v1/campaigns/{}/items/page/{id}", g.c);
    assert_eq!(
        g.f.call(
            "PUT",
            &endpoint,
            json!({"baseRevision":0,"payload":page("Original")}),
            g.owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, current) =
        g.f.call(
            "PUT",
            &endpoint,
            json!({"baseRevision":0,"payload":page("Stale")}),
            g.owner["token"].as_str(),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(current["payload"]["title"], "Original");
    let (_, deleted) =
        g.f.call(
            "DELETE",
            &format!("{endpoint}?baseRevision=1"),
            Value::Null,
            g.owner["token"].as_str(),
        )
        .await;
    assert_eq!(deleted["revision"], 2);
    let (_, own) =
        g.f.call(
            "GET",
            &g.path("page", &id),
            Value::Null,
            g.owner["token"].as_str(),
        )
        .await;
    assert_eq!(own["payload"]["title"], "Original");
    assert_eq!(own["visibility"], "private");
    assert_eq!(
        g.feed(&g.reader, "").await.1["items"][0]["payload"],
        Value::Null
    );
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!("{}?baseRevision=2", g.path("page", &id)),
            Value::Null,
            g.owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, recreated) =
        g.f.call(
            "PUT",
            &endpoint,
            json!({"baseRevision":0,"payload":page("Recreated")}),
            g.owner["token"].as_str(),
        )
        .await;
    assert_eq!(recreated["revision"], 4);
    assert_eq!(
        g.f.call(
            "PUT",
            &endpoint,
            json!({"baseRevision":1,"payload":page("Old draft")}),
            g.owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn removed_author_loses_access_and_shared_copies_stay_private_after_rejoin() {
    let g = Group::new().await;
    let id = Uuid::new_v4().to_string();
    g.save(
        &g.player,
        "page",
        &id,
        page("Attributed claim"),
        Some("members"),
        0,
    )
    .await;
    let member = format!(
        "/api/v1/campaigns/{}/members/{}?baseRevision=1",
        g.c,
        g.player["accountId"].as_str().unwrap()
    );
    assert_eq!(
        g.f.call("DELETE", &member, Value::Null, g.owner["token"].as_str())
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(g.feed(&g.player, "").await.0, StatusCode::FORBIDDEN);
    assert_eq!(g.feed(&g.reader, "").await.1["items"][0]["deleted"], true);
    let (_, i) =
        g.f.call(
            "POST",
            &format!("/api/v1/campaigns/{}/invites", g.c),
            json!({"role":"player"}),
            g.owner["token"].as_str(),
        )
        .await;
    g.f.call(
        "POST",
        "/api/v1/campaigns/join",
        json!({"inviteCode":i["code"]}),
        g.player["token"].as_str(),
    )
    .await;
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("page", &id),
            Value::Null,
            g.player["token"].as_str()
        )
        .await
        .1["visibility"],
        "private"
    );
    assert_eq!(
        g.f.call(
            "GET",
            &g.path("page", &id),
            Value::Null,
            g.reader["token"].as_str()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn original_images_are_private_until_explicitly_referenced_and_revocable() {
    let g = Group::new().await;
    let bytes = image_bytes(ImageFormat::Png);
    let uploaded = upload(&g, &g.player, &bytes, "image/png").await;
    let id = uploaded["id"].as_str().unwrap();
    let download = format!("/api/v1/campaigns/{}/attachments/{id}", g.c);
    assert_eq!(
        raw(
            &g.f,
            "GET",
            &download,
            g.player["token"].as_str(),
            "application/json",
            "",
            vec![]
        )
        .await
        .1,
        bytes
    );
    assert_eq!(
        raw(
            &g.f,
            "GET",
            &download,
            g.owner["token"].as_str(),
            "application/json",
            "",
            vec![]
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let content = Uuid::new_v4().to_string();
    let image = json!({"attachmentId":id,"caption":"Explicitly shared original"});
    assert_eq!(
        g.save(&g.player, "image", &content, image, Some("members"), 0)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        raw(
            &g.f,
            "GET",
            &download,
            g.reader["token"].as_str(),
            "application/json",
            "",
            vec![]
        )
        .await
        .1,
        bytes
    );
    assert_eq!(
        g.f.call("DELETE", &download, Value::Null, g.player["token"].as_str())
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!("{}/sharing?baseRevision=1", g.path("image", &content)),
            Value::Null,
            g.owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        raw(
            &g.f,
            "GET",
            &download,
            g.reader["token"].as_str(),
            "application/json",
            "",
            vec![]
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        raw(
            &g.f,
            "GET",
            &download,
            g.player["token"].as_str(),
            "application/json",
            "",
            vec![]
        )
        .await
        .1,
        bytes
    );
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!("{}?baseRevision=2", g.path("image", &content)),
            Value::Null,
            g.player["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        g.f.call("DELETE", &download, Value::Null, g.player["token"].as_str())
            .await
            .0,
        StatusCode::NO_CONTENT
    );
}
#[tokio::test]
async fn image_validation_rejects_spoofed_type_bad_checksum_scripts_corruption_and_unauthorized_bodies()
 {
    let g = Group::new().await;
    let path = format!("/api/v1/campaigns/{}/attachments", g.c);
    let good = image_bytes(ImageFormat::Png);
    for (bytes, mime, checksum) in [
        (
            good.clone(),
            "image/jpeg",
            format!("{:x}", Sha256::digest(&good)),
        ),
        (good.clone(), "image/png", "0".repeat(64)),
        (
            b"<svg><script>alert(1)</script></svg>".to_vec(),
            "image/png",
            format!(
                "{:x}",
                Sha256::digest(b"<svg><script>alert(1)</script></svg>")
            ),
        ),
        (
            good[..20].to_vec(),
            "image/png",
            format!("{:x}", Sha256::digest(&good[..20])),
        ),
    ] {
        assert_eq!(
            raw(
                &g.f,
                "POST",
                &path,
                g.owner["token"].as_str(),
                mime,
                &checksum,
                bytes
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        raw(
            &g.f,
            "POST",
            &path,
            None,
            "image/png",
            "",
            vec![0; 8 * 1024 * 1024 + 1]
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        raw(
            &g.f,
            "POST",
            &path,
            g.reader["token"].as_str(),
            "image/png",
            "",
            good.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        raw(
            &g.f,
            "POST",
            &path,
            g.owner["token"].as_str(),
            "image/png",
            "",
            vec![0; 8 * 1024 * 1024 + 1]
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    for (format, mime) in [
        (ImageFormat::Jpeg, "image/jpeg"),
        (ImageFormat::WebP, "image/webp"),
    ] {
        let bytes = image_bytes(format);
        let attachment = upload(&g, &g.owner, &bytes, mime).await;
        assert_eq!(
            attachment["sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
    }
}
#[tokio::test]
async fn attachment_reference_cannot_reuse_another_users_original_or_escape_campaign() {
    let g = Group::new().await;
    let bytes = image_bytes(ImageFormat::Png);
    let attachment = upload(&g, &g.player, &bytes, "image/png").await;
    let id = Uuid::new_v4().to_string();
    assert_eq!(
        g.save(
            &g.owner,
            "image",
            &id,
            json!({"attachmentId":attachment["id"],"caption":"Forged ownership"}),
            Some("members"),
            0
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let other =
        g.f.state
            .database
            .create_campaign(
                g.owner["token"].as_str().unwrap(),
                &chronexus_server::campaigns::CreateCampaign {
                    name: "Other".into(),
                },
            )
            .unwrap();
    let path = format!(
        "/api/v1/campaigns/{}/attachments/{}",
        other.id,
        attachment["id"].as_str().unwrap()
    );
    assert_eq!(
        raw(
            &g.f,
            "GET",
            &path,
            g.owner["token"].as_str(),
            "image/png",
            "",
            vec![]
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn feed_and_current_content_lists_are_bounded_and_survive_database_reopen() {
    let g = Group::new().await;
    let sample = serde_json::to_string(&page("Synthetic")).unwrap();
    g.f.state
        .database
        .with(|conn| {
            let tx = conn.transaction()?;
            for n in 1..=1001 {
                let id = format!("00000000-0000-4000-8000-{n:012x}");
                tx.execute(
                    "INSERT INTO content VALUES(?1,'page',?2,?3,1,'members',?4,0,1)",
                    rusqlite::params![g.c, id, g.owner["accountId"].as_str().unwrap(), sample],
                )?;
                tx.execute(
                    "INSERT INTO published_items VALUES(?1,'page',?2,1,0,?3,1)",
                    rusqlite::params![g.c, id, n],
                )?;
            }
            tx.execute("UPDATE campaigns SET next_seq=1001 WHERE id=?1", [&g.c])?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    let (_, first) = g.feed(&g.reader, "").await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1000);
    assert_eq!(first["more"], true);
    let (_, last) = g.feed(&g.reader, first["cursor"].as_str().unwrap()).await;
    assert_eq!(last["items"].as_array().unwrap().len(), 1);
    assert_eq!(last["more"], false);
    let (_, list) =
        g.f.call(
            "GET",
            &format!("/api/v1/campaigns/{}/content", g.c),
            Value::Null,
            g.reader["token"].as_str(),
        )
        .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 50);
    assert_eq!(list["more"], true);
    let db = chronexus_server::db::Database::open(&g.f.state.config.database).unwrap();
    assert_eq!(
        db.changes(g.reader["token"].as_str().unwrap(), &g.c, "1000")
            .unwrap()
            .items
            .len(),
        1
    );
}

#[tokio::test]
async fn simultaneous_edits_return_one_conflict_without_losing_an_acknowledged_revision() {
    let g = Group::new().await;
    let id = Uuid::new_v4().to_string();
    g.save(&g.owner, "page", &id, page("Original"), Some("members"), 0)
        .await;
    let (one, two) = tokio::join!(
        g.save(
            &g.owner,
            "page",
            &id,
            page("First device"),
            Some("members"),
            1
        ),
        g.save(
            &g.owner,
            "page",
            &id,
            page("Second device"),
            Some("members"),
            1
        )
    );
    let mut codes = [one.0.as_u16(), two.0.as_u16()];
    codes.sort();
    assert_eq!(codes, [200, 409]);
    let conflict = if one.0 == StatusCode::CONFLICT {
        one.1
    } else {
        two.1
    };
    assert_eq!(conflict["revision"], 2);
    let (_, current) =
        g.f.call(
            "GET",
            &g.path("page", &id),
            Value::Null,
            g.owner["token"].as_str(),
        )
        .await;
    assert_eq!(current["payload"], conflict["payload"]);
}

#[tokio::test]
async fn feed_byte_limit_is_below_desktop_response_limit() {
    let g = Group::new().await;
    let mut large = page("Large shared note");
    large["document"]["blocks"][0]["content"][0]["text"] = json!("x".repeat(1_700_000));
    for _ in 0..4 {
        assert_eq!(
            g.save(
                &g.owner,
                "page",
                &Uuid::new_v4().to_string(),
                large.clone(),
                Some("members"),
                0
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let (_, first) = g.feed(&g.reader, "").await;
    assert!(serde_json::to_vec(&first).unwrap().len() < 8 * 1024 * 1024);
    assert_eq!(first["items"].as_array().unwrap().len(), 3);
    assert_eq!(first["more"], true);
    assert_eq!(
        g.feed(&g.reader, first["cursor"].as_str().unwrap()).await.1["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn attachment_dimension_and_count_limits_and_content_quota_are_enforced() {
    let g = Group::new().await;
    let mut large = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(8193, 1)
        .write_to(&mut large, ImageFormat::Png)
        .unwrap();
    let bytes = large.into_inner();
    let path = format!("/api/v1/campaigns/{}/attachments", g.c);
    assert_eq!(
        raw(
            &g.f,
            "POST",
            &path,
            g.owner["token"].as_str(),
            "image/png",
            &format!("{:x}", Sha256::digest(&bytes)),
            bytes
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let bytes = image_bytes(ImageFormat::Png);
    let checksum = format!("{:x}", Sha256::digest(&bytes));
    g.f.state
        .database
        .with(|conn| {
            let tx = conn.transaction()?;
            for _ in 0..512 {
                tx.execute(
                    "INSERT INTO attachments VALUES(?1,?2,?3,?4,'image/png',2,2,?5,?6,1)",
                    rusqlite::params![
                        Uuid::new_v4().to_string(),
                        g.c,
                        g.owner["accountId"].as_str().unwrap(),
                        checksum,
                        bytes.len() as i64,
                        bytes
                    ],
                )?;
            }
            let payload = page("Quota fixture").to_string();
            for n in 0..5000 {
                tx.execute(
                    "INSERT INTO content VALUES(?1,'page',?2,?3,1,'private',?4,0,1)",
                    rusqlite::params![
                        g.c,
                        format!("00000000-0000-4000-8000-{n:012x}"),
                        g.owner["accountId"].as_str().unwrap(),
                        payload
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        raw(
            &g.f,
            "POST",
            &path,
            g.owner["token"].as_str(),
            "image/png",
            &checksum,
            bytes
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        g.save(
            &g.owner,
            "page",
            &Uuid::new_v4().to_string(),
            page("One too many"),
            None,
            0
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[test]
fn migration_refuses_another_app_database_even_when_its_version_matches() {
    let directory = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    let path = directory.path().join("not-server.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("CREATE TABLE campaigns(id TEXT PRIMARY KEY,name TEXT)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO campaigns VALUES('private-id','PRIVATE-MARKER')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    assert!(chronexus_server::db::Database::open(&path).is_err());
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row("SELECT name FROM campaigns", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "PRIVATE-MARKER"
    );
    assert!(conn.prepare("SELECT next_seq FROM campaigns").is_err());
    assert_eq!(
        conn.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
}
