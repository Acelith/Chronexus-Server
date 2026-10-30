//! M25.7: backups, verification, restore on a clean instance and failed-upgrade recovery.
mod common;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use chronexus_server::{
    AppState,
    backup::{create, restore, verify},
    db::Database,
    routes,
};
use common::Fixture;
use http_body_util::BodyExt;
use image::{DynamicImage, ImageFormat};
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, net::SocketAddr, path::Path};
use tower::ServiceExt;

struct Seeded {
    f: Fixture,
    campaign: String,
    owner: Value,
    removed: Value,
    png: Vec<u8>,
}
async fn upload(f: &Fixture, campaign: &str, token: &str, bytes: &[u8]) -> StatusCode {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/campaigns/{campaign}/attachments"))
        .header("host", "127.0.0.1:8080")
        .header("content-type", "image/png")
        .header("x-content-sha256", format!("{:x}", Sha256::digest(bytes)))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(bytes.to_vec()))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:54321".parse::<SocketAddr>().unwrap(),
    ));
    let response = f.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let _ = response.into_body().collect().await;
    status
}
async fn seeded() -> Seeded {
    let f = Fixture::new();
    let owner = f.account("owner").await;
    let removed = f.account("removed").await;
    let (_, campaign) = f
        .call(
            "POST",
            "/api/v1/campaigns",
            json!({"name":"Synthetic campaign"}),
            owner["token"].as_str(),
        )
        .await;
    let campaign = campaign["id"].as_str().unwrap().to_string();
    let (_, invite) = f
        .call(
            "POST",
            &format!("/api/v1/campaigns/{campaign}/invites"),
            json!({"role":"player"}),
            owner["token"].as_str(),
        )
        .await;
    f.call(
        "POST",
        "/api/v1/campaigns/join",
        json!({"inviteCode":invite["code"]}),
        removed["token"].as_str(),
    )
    .await;
    let id = uuid::Uuid::new_v4().to_string();
    let page = json!({"title":"Synthetic","kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"b","type":"paragraph","props":{},"content":[{"type":"text","text":"Synthetic text","styles":{}}],"children":[]}]}});
    assert_eq!(
        f.call(
            "PUT",
            &format!("/api/v1/campaigns/{campaign}/content/page/{id}"),
            json!({"baseRevision":0,"visibility":"members","payload":page}),
            owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut out = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(3, 3)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    let png = out.into_inner();
    assert_eq!(
        upload(&f, &campaign, owner["token"].as_str().unwrap(), &png).await,
        StatusCode::CREATED
    );
    // The second player is removed before the backup: that revocation must survive a restore.
    let (_, members) = f
        .call(
            "GET",
            &format!("/api/v1/campaigns/{campaign}/members"),
            Value::Null,
            owner["token"].as_str(),
        )
        .await;
    let member = members
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "player")
        .unwrap();
    assert_eq!(
        f.call(
            "DELETE",
            &format!(
                "/api/v1/campaigns/{campaign}/members/{}?baseRevision={}",
                member["accountId"].as_str().unwrap(),
                member["revision"]
            ),
            Value::Null,
            owner["token"].as_str()
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    Seeded {
        f,
        campaign,
        owner,
        removed,
        png,
    }
}
fn database_path(s: &Seeded) -> std::path::PathBuf {
    s.f.state.config.database.clone()
}
fn corrupt(path: &Path) {
    let mut bytes = fs::read(path).unwrap();
    let middle = bytes.len() / 2;
    for byte in &mut bytes[middle..middle + 4096] {
        *byte ^= 0xff;
    }
    fs::write(path, bytes).unwrap();
}

#[tokio::test]
async fn a_backup_taken_while_running_restores_on_a_clean_instance_with_revocations_intact() {
    let s = seeded().await;
    let dir = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    let file = dir.path().join("backup.sqlite3");
    let report = create(&database_path(&s), &file).unwrap();
    assert_eq!(
        (
            report.accounts,
            report.campaigns,
            report.content,
            report.attachments
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(verify(&file).unwrap(), report);
    // Backups never overwrite anything.
    assert!(
        create(&database_path(&s), &file)
            .unwrap_err()
            .contains("already exists")
    );

    // A clean instance: a new, empty data directory.
    let clean = dir.path().join("clean").join("server.sqlite3");
    let (restored, ended, kept) = restore(&clean, &file).unwrap();
    assert_eq!(restored, report);
    assert_eq!(ended, 2, "both sessions of the backup end");
    assert!(kept.is_none());
    let mut config = (*s.f.state.config).clone();
    config.database = clean.clone();
    let state = AppState::new(config).unwrap();
    let app = routes::router(state.clone());
    // Old sessions are dead; a new sign-in works and sees the data and the removal.
    let owner_token = s.owner["token"].as_str();
    assert_eq!(
        common::call(
            app.clone(),
            "GET",
            "/api/v1/campaigns",
            Value::Null,
            owner_token,
            "127.0.0.1:8080",
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, signed) = common::call(
        app.clone(),
        "POST",
        "/api/v1/auth/login",
        json!({"username":"owner","password":"synthetic-password-123","device":"after restore"}),
        None,
        "127.0.0.1:8080",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = signed["token"].as_str();
    let (_, feed) = common::call(
        app.clone(),
        "GET",
        &format!("/api/v1/campaigns/{}/content", s.campaign),
        Value::Null,
        token,
        "127.0.0.1:8080",
        None,
    )
    .await;
    assert_eq!(feed["items"].as_array().unwrap().len(), 1);
    let stored: Vec<u8> = state
        .database
        .with(|conn| Ok(conn.query_row("SELECT original FROM attachments", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(stored, s.png, "uploaded originals survive byte for byte");
    let (_, members) = common::call(
        app.clone(),
        "GET",
        &format!("/api/v1/campaigns/{}/members", s.campaign),
        Value::Null,
        token,
        "127.0.0.1:8080",
        None,
    )
    .await;
    assert_eq!(
        members.as_array().unwrap().len(),
        1,
        "the removed member stays removed"
    );
    let (status, _) = common::call(
        app,
        "POST",
        "/api/v1/auth/login",
        json!({"username":"removed","password":"synthetic-password-123","device":"d"}),
        None,
        "127.0.0.1:8080",
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "accounts survive; only the membership is gone"
    );
    let _ = &s.removed;
}

#[tokio::test]
async fn damaged_foreign_newer_or_tampered_backups_are_refused_and_never_replace_the_database() {
    let s = seeded().await;
    let dir = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    let good = dir.path().join("good.sqlite3");
    create(&database_path(&s), &good).unwrap();
    let bytes = |name: &str| dir.path().join(name);

    let damaged = bytes("damaged.sqlite3");
    fs::copy(&good, &damaged).unwrap();
    corrupt(&damaged);
    assert!(verify(&damaged).is_err());

    let foreign = bytes("foreign.sqlite3");
    Connection::open(&foreign)
        .unwrap()
        .execute_batch("CREATE TABLE notes(id INTEGER); INSERT INTO notes VALUES(1);")
        .unwrap();
    assert!(
        verify(&foreign)
            .unwrap_err()
            .contains("not a Chronexus Server backup")
    );

    let newer = bytes("newer.sqlite3");
    fs::copy(&good, &newer).unwrap();
    Connection::open(&newer)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    assert!(verify(&newer).unwrap_err().contains("supports up to"));

    // Same length, different bytes: the stored checksum catches it.
    let tampered = bytes("tampered.sqlite3");
    fs::copy(&good, &tampered).unwrap();
    Connection::open(&tampered)
        .unwrap()
        .execute(
            "UPDATE attachments SET original=zeroblob(length(original))",
            [],
        )
        .unwrap();
    assert!(verify(&tampered).unwrap_err().contains("checksum"));
    assert!(verify(&bytes("missing.sqlite3")).is_err());
    assert!(verify(dir.path()).is_err());

    // A refused restore leaves the live database exactly as it was.
    let live = dir.path().join("live").join("server.sqlite3");
    restore(&live, &good).unwrap();
    let before = fs::read(&live).unwrap();
    for bad in [&damaged, &foreign, &newer, &tampered] {
        assert!(restore(&live, bad).is_err());
        assert_eq!(fs::read(&live).unwrap(), before);
    }
    // A good restore over an existing database keeps the previous one next to it.
    let (_, _, kept) = restore(&live, &good).unwrap();
    let kept = kept.unwrap();
    assert!(kept.exists() && kept.to_string_lossy().contains("pre-restore"));
}

fn schema_five(path: &Path) {
    let conn = Connection::open(path).unwrap();
    for migration in [
        include_str!("../migrations/001_accounts.sql"),
        include_str!("../migrations/002_campaigns.sql"),
        include_str!("../migrations/003_content.sql"),
        include_str!("../migrations/004_save_receipts.sql"),
        include_str!("../migrations/005_sync_log.sql"),
    ] {
        conn.execute_batch(migration).unwrap();
    }
    conn.pragma_update(None, "application_id", 0x43584e53)
        .unwrap();
    conn.pragma_update(None, "user_version", 5).unwrap();
    conn.execute(
        "INSERT INTO accounts VALUES('acc','synthetic','Synthetic','hash',1,0)",
        [],
    )
    .unwrap();
}

#[test]
fn an_upgrade_first_copies_the_old_database_and_a_failed_one_changes_nothing() {
    let dir = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    // Successful upgrade: a verified copy of schema 5 exists next to the upgraded database.
    let ok = dir.path().join("ok.sqlite3");
    schema_five(&ok);
    drop(Database::open(&ok).unwrap());
    let copy = dir.path().join("ok.sqlite3.pre-upgrade-v5");
    assert_eq!(verify(&copy).unwrap().schema, 5);
    assert_eq!(
        verify(&ok).unwrap().schema,
        chronexus_server::db::LATEST_SCHEMA
    );

    // Failed upgrade: the migration cannot run, the database is untouched, the copy restores it.
    let bad = dir.path().join("bad.sqlite3");
    schema_five(&bad);
    Connection::open(&bad)
        .unwrap()
        .execute_batch("ALTER TABLE campaigns ADD COLUMN deleted_at INTEGER;")
        .unwrap();
    let before = Connection::open(&bad)
        .unwrap()
        .query_row("SELECT count(*) FROM accounts", [], |r| r.get::<_, i64>(0))
        .unwrap();
    let error = Database::open(&bad).err().unwrap();
    assert!(error.contains("rolled back"), "{error}");
    let conn = Connection::open(&bad).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM accounts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        before
    );
    let saved = dir.path().join("bad.sqlite3.pre-upgrade-v5");
    assert_eq!(verify(&saved).unwrap().schema, 5);
    // The copy of the good upgrade restores on a new instance, which upgrades it again.
    let rollback = dir.path().join("rollback").join("server.sqlite3");
    let (report, _, _) = restore(&rollback, &copy).unwrap();
    assert_eq!(report.schema, 5);
    let newer = dir.path().join("newer.sqlite3");
    schema_five(&newer);
    Connection::open(&newer)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    assert!(
        Database::open(&newer)
            .err()
            .unwrap()
            .contains("newer than this server")
    );
}

#[test]
fn an_incomplete_or_failing_restore_never_replaces_the_active_database() {
    let dir = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    let live = dir.path().join("live").join("server.sqlite3");
    let good = dir.path().join("good.sqlite3");
    schema_five(&good);
    restore(&live, &good).unwrap();
    let before = fs::read(&live).unwrap();
    let leftovers = || {
        fs::read_dir(live.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".re"))
            .count()
    };

    // Integrity-valid and of the latest schema, but a table is missing.
    let incomplete = dir.path().join("incomplete.sqlite3");
    schema_five(&incomplete);
    drop(Database::open(&incomplete).unwrap());
    Connection::open(&incomplete)
        .unwrap()
        .execute_batch("DROP TABLE rate_buckets;")
        .unwrap();
    assert!(
        verify(&incomplete)
            .unwrap_err()
            .contains("complete structure")
    );
    assert!(restore(&live, &incomplete).is_err());
    assert_eq!(fs::read(&live).unwrap(), before);

    // An older backup whose sessions cannot be ended: the failure happens on the staging
    // copy, before activation, so no old bearer can become valid again.
    let failing = dir.path().join("failing.sqlite3");
    schema_five(&failing);
    Connection::open(&failing)
        .unwrap()
        .execute_batch(
            "INSERT INTO sessions VALUES('s','hash','acc','old device',0,0,9999999999,9999999999,NULL);
             CREATE TRIGGER keep_sessions BEFORE UPDATE ON sessions BEGIN SELECT RAISE(ABORT,'blocked'); END;",
        )
        .unwrap();
    assert_eq!(verify(&failing).unwrap().schema, 5);
    assert!(restore(&live, &failing).is_err());
    assert_eq!(fs::read(&live).unwrap(), before);
    assert_eq!(leftovers(), 0);
    // Neither refused restore moved the active database aside.
    let kept = fs::read_dir(live.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("pre-restore-"))
        .count();
    assert_eq!(kept, 0);
}
