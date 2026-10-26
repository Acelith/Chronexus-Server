mod common;
use axum::http::StatusCode;
use chronexus_server::{auth, db::Database};
use common::Fixture;
use serde_json::{Value, json};

async fn create(f: &Fixture, token: &str) -> String {
    let (status, c) = f
        .call(
            "POST",
            "/api/v1/campaigns",
            json!({"name":"Synthetic campaign"}),
            Some(token),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{c}");
    c["id"].as_str().unwrap().into()
}
async fn invite(f: &Fixture, token: &str, campaign: &str, role: &str) -> Value {
    let (status, i) = f
        .call(
            "POST",
            &format!("/api/v1/campaigns/{campaign}/invites"),
            json!({"role":role}),
            Some(token),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{i}");
    i
}
#[tokio::test]
async fn three_accounts_join_with_expected_roles_and_no_cross_campaign_metadata() {
    let f = Fixture::new();
    let owner = f.account("owner").await;
    let player = f.account("player").await;
    let reader = f.account("reader").await;
    let outsider = f.account("outsider").await;
    let ot = owner["token"].as_str().unwrap();
    let campaign = create(&f, ot).await;
    for (account, role) in [(&player, "player"), (&reader, "reader")] {
        let i = invite(&f, ot, &campaign, role).await;
        let (status, joined) = f
            .call(
                "POST",
                "/api/v1/campaigns/join",
                json!({"inviteCode":i["code"]}),
                account["token"].as_str(),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(joined["role"], role);
        assert_eq!(
            f.call(
                "POST",
                &format!("/api/v1/campaigns/{campaign}/invites"),
                json!({"role":"player"}),
                account["token"].as_str()
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let (status, members) = f
        .call(
            "GET",
            &format!("/api/v1/campaigns/{campaign}/members"),
            Value::Null,
            reader["token"].as_str(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(members.as_array().unwrap().len(), 3);
    assert!(!members.to_string().contains("username"));
    let (_, list) = f
        .call(
            "GET",
            "/api/v1/campaigns",
            Value::Null,
            outsider["token"].as_str(),
        )
        .await;
    assert_eq!(list, json!([]));
    for suffix in ["", "/members", "/invites"] {
        let (status, error) = f
            .call(
                "GET",
                &format!("/api/v1/campaigns/{campaign}{suffix}"),
                Value::Null,
                outsider["token"].as_str(),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(!error.to_string().contains("Synthetic"));
    }
}
#[tokio::test]
async fn campaign_invitation_registers_and_joins_atomically_without_storing_code() {
    let f = Fixture::new();
    let o = f.account("owner").await;
    let token = o["token"].as_str().unwrap();
    let c = create(&f, token).await;
    let i = invite(&f, token, &c, "reader").await;
    let body = json!({"inviteCode":i["code"],"username":"newreader","displayName":"New Reader","password":"synthetic-password-123","device":"laptop"});
    let (status, account) = f
        .call("POST", "/api/v1/auth/redeem", body.clone(), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, campaigns) = f
        .call(
            "GET",
            "/api/v1/campaigns",
            Value::Null,
            account["token"].as_str(),
        )
        .await;
    assert_eq!(campaigns[0]["role"], "reader");
    assert_eq!(
        f.call("POST", "/api/v1/auth/redeem", body, None).await.0,
        StatusCode::GONE
    );
    let hash: String = f
        .state
        .database
        .with(|conn| {
            Ok(conn.query_row(
                "SELECT code_hash FROM campaign_invites WHERE id=?1",
                [i["id"].as_str().unwrap()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(hash, auth::digest(i["code"].as_str().unwrap()));
}
#[tokio::test]
async fn invitations_expire_revoke_and_cannot_be_reused_or_escalate_owner() {
    let f = Fixture::new();
    let o = f.account("owner").await;
    let p = f.account("player").await;
    let token = o["token"].as_str().unwrap();
    let c = create(&f, token).await;
    assert_eq!(
        f.call(
            "POST",
            &format!("/api/v1/campaigns/{c}/invites"),
            json!({"role":"owner"}),
            Some(token)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let expired = invite(&f, token, &c, "player").await;
    f.state
        .database
        .with(|conn| {
            conn.execute(
                "UPDATE campaign_invites SET expires_at=0 WHERE id=?1",
                [expired["id"].as_str().unwrap()],
            )?;
            Ok(())
        })
        .unwrap();
    let revoked = invite(&f, token, &c, "player").await;
    assert_eq!(
        f.call(
            "DELETE",
            &format!(
                "/api/v1/campaigns/{c}/invites/{}",
                revoked["id"].as_str().unwrap()
            ),
            Value::Null,
            Some(token)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    for i in [&expired, &revoked] {
        assert_eq!(
            f.call(
                "POST",
                "/api/v1/campaigns/join",
                json!({"inviteCode":i["code"]}),
                p["token"].as_str()
            )
            .await
            .0,
            StatusCode::GONE
        );
    }
    let valid = invite(&f, token, &c, "player").await;
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/campaigns/join",
            json!({"inviteCode":valid["code"]}),
            p["token"].as_str()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/campaigns/join",
            json!({"inviteCode":valid["code"]}),
            p["token"].as_str()
        )
        .await
        .0,
        StatusCode::GONE
    );
    let (_, listed) = f
        .call(
            "GET",
            &format!("/api/v1/campaigns/{c}/invites"),
            Value::Null,
            Some(token),
        )
        .await;
    assert!(!listed.to_string().contains(valid["code"].as_str().unwrap()));
}
#[tokio::test]
async fn concurrent_join_consumes_one_invite_once() {
    let f = Fixture::new();
    let o = f.account("owner").await;
    let a = f.account("alice").await;
    let b = f.account("bobby").await;
    let ot = o["token"].as_str().unwrap();
    let c = create(&f, ot).await;
    let i = invite(&f, ot, &c, "player").await;
    let body = json!({"inviteCode":i["code"]});
    let (one, two) = tokio::join!(
        f.call(
            "POST",
            "/api/v1/campaigns/join",
            body.clone(),
            a["token"].as_str()
        ),
        f.call("POST", "/api/v1/campaigns/join", body, b["token"].as_str())
    );
    let mut statuses = [one.0.as_u16(), two.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 410]);
}
#[tokio::test]
async fn role_changes_and_removal_reject_stale_requests_and_protect_owner() {
    let f = Fixture::new();
    let o = f.account("owner").await;
    let p = f.account("player").await;
    let ot = o["token"].as_str().unwrap();
    let pt = p["token"].as_str().unwrap();
    let c = create(&f, ot).await;
    let i = invite(&f, ot, &c, "player").await;
    f.call(
        "POST",
        "/api/v1/campaigns/join",
        json!({"inviteCode":i["code"]}),
        Some(pt),
    )
    .await;
    let path = format!(
        "/api/v1/campaigns/{c}/members/{}",
        p["accountId"].as_str().unwrap()
    );
    assert_eq!(
        f.call(
            "PATCH",
            &path,
            json!({"role":"reader","baseRevision":1}),
            Some(pt)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.call(
            "PATCH",
            &path,
            json!({"role":"reader","baseRevision":1}),
            Some(ot)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.call(
            "PATCH",
            &path,
            json!({"role":"player","baseRevision":1}),
            Some(ot)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        f.call(
            "DELETE",
            &format!("{path}?baseRevision=1"),
            Value::Null,
            Some(ot)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let owner_path = format!(
        "/api/v1/campaigns/{c}/members/{}",
        o["accountId"].as_str().unwrap()
    );
    assert_eq!(
        f.call(
            "DELETE",
            &format!("{owner_path}?baseRevision=1"),
            Value::Null,
            Some(ot)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.call(
            "PATCH",
            &owner_path,
            json!({"role":"player","baseRevision":1}),
            Some(ot)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.call(
            "DELETE",
            &format!("{path}?baseRevision=2"),
            Value::Null,
            Some(pt)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.call(
            "GET",
            &format!("/api/v1/campaigns/{c}"),
            Value::Null,
            Some(pt)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let i = invite(&f, ot, &c, "reader").await;
    f.call(
        "POST",
        "/api/v1/campaigns/join",
        json!({"inviteCode":i["code"]}),
        Some(pt),
    )
    .await;
    assert_eq!(
        f.call(
            "DELETE",
            &format!("{path}?baseRevision=1"),
            Value::Null,
            Some(ot)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn membership_quota_and_strict_fields_fail_without_consuming_invites() {
    let f = Fixture::new();
    let o = f.account("owner").await;
    let token = o["token"].as_str().unwrap();
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/campaigns",
            json!({"name":"Test","ownerId":"forged"}),
            Some(token)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.call("POST", "/api/v1/campaigns", json!({"name":""}), Some(token))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.call("POST", "/api/v1/campaigns", json!({"name":"Test"}), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for n in 0..100 {
        f.state
            .database
            .create_campaign(
                token,
                &chronexus_server::campaigns::CreateCampaign {
                    name: format!("Campaign {n}"),
                },
            )
            .unwrap();
    }
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/campaigns",
            json!({"name":"Over quota"}),
            Some(token)
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
}
#[test]
fn account_schema_upgrade_preserves_credentials_and_foreign_keys() {
    let directory = tempfile::tempdir_in("/private/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap();
    let path = directory.path().join("old.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("../migrations/001_accounts.sql"))
        .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    conn.execute(
        "INSERT INTO accounts VALUES('old-id','old-user','Old User','retained-hash',1,0)",
        [],
    )
    .unwrap();
    drop(conn);
    let db = Database::open(&path).unwrap();
    db.with(|conn| {
        let hash: String =
            conn.query_row("SELECT password_hash FROM accounts", [], |r| r.get(0))?;
        assert_eq!(hash, "retained-hash");
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            3
        );
        assert_eq!(
            conn.pragma_query_value(None, "foreign_keys", |r| r.get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
}
