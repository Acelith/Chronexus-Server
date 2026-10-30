mod common;
use axum::http::StatusCode;
use common::Fixture;
use serde_json::{Value, json};

#[tokio::test]
async fn table_seats_do_not_grant_campaign_access() {
    let f = Fixture::new();
    let gm = f.account("table-gm").await;
    let player = f.account("table-player").await;
    let outsider = f.account("table-outsider").await;
    let gm_token = gm["token"].as_str().unwrap();
    let player_token = player["token"].as_str().unwrap();
    let outsider_token = outsider["token"].as_str().unwrap();
    let (status, table) = f.call("POST", "/api/v1/tables", json!({"name":"Synthetic table"}), Some(gm_token)).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = table["id"].as_str().unwrap();
    let (_, campaign) = f.call("POST", "/api/v1/campaigns", json!({"name":"Private journal"}), Some(gm_token)).await;
    let campaign_id = campaign["id"].as_str().unwrap();
    assert_eq!(f.call("GET",&format!("/api/v1/tables/{id}"),Value::Null,Some(outsider_token)).await.0,StatusCode::FORBIDDEN);
    assert_eq!(f.call("POST",&format!("/api/v1/tables/{id}/invites"),json!({"role":"player"}),Some(player_token)).await.0,StatusCode::FORBIDDEN);
    let (status, invite) = f.call("POST",&format!("/api/v1/tables/{id}/invites"),json!({"role":"player"}),Some(gm_token)).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, joined) = f.call("POST","/api/v1/tables/join",json!({"inviteCode":invite["code"]}),Some(player_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(joined["role"], "player");
    assert_eq!(f.call("POST","/api/v1/tables/join",json!({"inviteCode":invite["code"]}),Some(outsider_token)).await.0,StatusCode::GONE);
    assert_eq!(f.call("GET",&format!("/api/v1/campaigns/{campaign_id}"),Value::Null,Some(player_token)).await.0,StatusCode::FORBIDDEN);
    assert_eq!(f.call("GET","/api/v1/campaigns",Value::Null,Some(player_token)).await.1,json!([]));
}
