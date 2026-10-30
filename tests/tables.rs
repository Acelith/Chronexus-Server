mod common;
use axum::http::StatusCode;
use common::Fixture;
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn table_seats_do_not_grant_campaign_access() {
    let f = Fixture::new();
    let gm_code = f.state.database.issue_invite_with_gm(None, true).unwrap();
    let (_,gm) = f.call("POST","/api/v1/auth/redeem",json!({"inviteCode":gm_code,"username":"table-gm","displayName":"Table GM","password":"synthetic-password-123","device":"test desktop"}),None).await;
    let player = f.account("table-player").await;
    let outsider = f.account("table-outsider").await;
    let gm_token = gm["token"].as_str().unwrap();
    let player_token = player["token"].as_str().unwrap();
    let outsider_token = outsider["token"].as_str().unwrap();
    let (status, table) = f
        .call(
            "POST",
            "/api/v1/tables",
            json!({"name":"Synthetic table"}),
            Some(gm_token),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = table["id"].as_str().unwrap();
    let (_, campaign) = f
        .call(
            "POST",
            "/api/v1/campaigns",
            json!({"name":"Private journal"}),
            Some(gm_token),
        )
        .await;
    let campaign_id = campaign["id"].as_str().unwrap();
    assert_eq!(
        f.call(
            "GET",
            &format!("/api/v1/tables/{id}"),
            Value::Null,
            Some(outsider_token)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.call(
            "POST",
            &format!("/api/v1/tables/{id}/invites"),
            json!({"role":"player"}),
            Some(player_token)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, invite) = f
        .call(
            "POST",
            &format!("/api/v1/tables/{id}/invites"),
            json!({"role":"player"}),
            Some(gm_token),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, joined) = f
        .call(
            "POST",
            "/api/v1/tables/join",
            json!({"inviteCode":invite["code"]}),
            Some(player_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(joined["role"], "player");
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/tables/join",
            json!({"inviteCode":invite["code"]}),
            Some(outsider_token)
        )
        .await
        .0,
        StatusCode::GONE
    );
    assert_eq!(
        f.call(
            "GET",
            &format!("/api/v1/campaigns/{campaign_id}"),
            Value::Null,
            Some(player_token)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.call("GET", "/api/v1/campaigns", Value::Null, Some(player_token))
            .await
            .1,
        json!([])
    );
}

#[tokio::test]
async fn gm_sees_only_the_authors_explicit_shared_item() {
    let f = Fixture::new();
    let code = f.state.database.issue_invite_with_gm(None, true).unwrap();
    let (_,gm)=f.call("POST","/api/v1/auth/redeem",json!({"inviteCode":code,"username":"gm-grants","displayName":"GM","password":"synthetic-password-123","device":"test desktop"}),None).await;
    let player = f.account("grant-player").await;
    let stranger = f.account("grant-stranger").await;
    let gt = gm["token"].as_str().unwrap();
    let pt = player["token"].as_str().unwrap();
    let (_, table) = f
        .call("POST", "/api/v1/tables", json!({"name":"Table"}), Some(gt))
        .await;
    let id = table["id"].as_str().unwrap();
    let (_, invite) = f
        .call(
            "POST",
            &format!("/api/v1/tables/{id}/invites"),
            json!({"role":"player"}),
            Some(gt),
        )
        .await;
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/tables/join",
            json!({"inviteCode":invite["code"]}),
            Some(pt)
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, campaign) = f
        .call(
            "POST",
            "/api/v1/campaigns",
            json!({"name":"Journal"}),
            Some(pt),
        )
        .await;
    let cid = campaign["id"].as_str().unwrap();
    let public = Uuid::new_v4().to_string();
    let private = Uuid::new_v4().to_string();
    let page = |title| json!({"title":title,"kind":"npc","status":"active","aliases":[],"document":{"version":1,"blocks":[{"id":"block-1","type":"paragraph","props":{},"content":[],"children":[]}]}});
    for (item, visibility) in [(&public, "members"), (&private, "private")] {
        assert_eq!(
            f.call(
                "PUT",
                &format!("/api/v1/campaigns/{cid}/content/page/{item}"),
                json!({"baseRevision":0,"visibility":visibility,"payload":page(visibility)}),
                Some(pt)
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let path = format!("/api/v1/tables/{id}/journal-grants");
    let key = |item: &str| json!({"campaignId":cid,"kind":"page","itemId":item});
    assert_eq!(
        f.call("POST", &path, key(&private), Some(pt)).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.call("POST", &path, key(&public), stranger["token"].as_str())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.call("POST", &path, key(&public), Some(pt)).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, items) = f.call("GET", &path, Value::Null, Some(gt)).await;
    assert_eq!(items.as_array().unwrap().len(), 1);
    assert_eq!(items[0]["itemId"], public);
    assert!(!items.to_string().contains("private"));
    assert_eq!(
        f.call("GET", &path, Value::Null, Some(pt)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.call("DELETE", &path, key(&public), Some(pt)).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.call("GET", &path, Value::Null, Some(gt)).await.1,
        json!([])
    );
    assert_eq!(
        f.call(
            "DELETE",
            &format!(
                "/api/v1/tables/{id}/seats/{}",
                player["accountId"].as_str().unwrap()
            ),
            Value::Null,
            Some(gt)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.call(
            "GET",
            &format!("/api/v1/tables/{id}"),
            Value::Null,
            Some(pt)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn table_code_registers_new_player_but_not_a_gm() {
    let f = Fixture::new();
    let gm_code = f.state.database.issue_invite_with_gm(None, true).unwrap();
    let (_,gm)=f.call("POST","/api/v1/auth/redeem",json!({"inviteCode":gm_code,"username":"signup-gm","displayName":"GM","password":"synthetic-password-123","device":"browser"}),None).await;
    let (_, table) = f
        .call(
            "POST",
            "/api/v1/tables",
            json!({"name":"Signup table"}),
            gm["token"].as_str(),
        )
        .await;
    let id = table["id"].as_str().unwrap();
    let (_, invite) = f
        .call(
            "POST",
            &format!("/api/v1/tables/{id}/invites"),
            json!({"role":"player"}),
            gm["token"].as_str(),
        )
        .await;
    let (status,new_player)=f.call("POST","/api/v1/auth/redeem",json!({"inviteCode":invite["code"],"username":"new-table-player","displayName":"New player","password":"synthetic-password-123","device":"browser"}),None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        f.call(
            "GET",
            &format!("/api/v1/tables/{id}"),
            Value::Null,
            new_player["token"].as_str()
        )
        .await
        .1["role"],
        "player"
    );
    assert_eq!(
        f.call(
            "POST",
            "/api/v1/tables",
            json!({"name":"Denied"}),
            new_player["token"].as_str()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
