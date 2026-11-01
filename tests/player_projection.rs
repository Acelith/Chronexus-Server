mod common;
use axum::http::StatusCode;
use common::table::TableFixture;
use serde_json::{Value, json};
#[tokio::test]
async fn player_projection_is_explicit_owned_private_and_preserves_table_authority() {
    let t = TableFixture::new().await;
    let scene = t.scene().await;
    t.activate(&scene).await;
    let token = t.token(&scene, &t.player, false).await;
    let mut projection = json!({"tokenId":token["id"],"characterId":uuid::Uuid::new_v4().to_string(),"name":"Chosen projection","speed":{"value":9,"unit":"m"},"portraitId":null,"vitals":{"hp":8,"maxHp":12,"ac":15,"showHp":false}});
    for other in [&t.other, &t.gm, &t.spectator] {
        assert_eq!(
            t.command(
                other,
                json!({"type":"projectToken","projection":projection})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        t.command(
            &t.player,
            json!({"type":"projectToken","projection":projection})
        )
        .await
        .0,
        StatusCode::OK
    );
    let own = t.view(&t.player).await.1;
    assert_eq!(own["tokens"][0]["name"], "Chosen projection");
    assert_eq!(own["tokens"][0]["speed"]["value"], 9.0);
    assert_eq!(own["vitals"][0]["hp"], 8);
    let other = t.view(&t.other).await.1;
    assert!(other["vitals"][0].get("hp").is_none());
    assert_eq!(other["tokens"][0]["characterId"], Value::Null);
    // A later HP-only projection preserves the separately chosen AC and disclosure.
    projection["name"] = Value::Null;
    projection["speed"] = Value::Null;
    projection["vitals"]["ac"] = Value::Null;
    projection["vitals"]["hp"] = json!(7);
    projection["vitals"]["showHp"] = json!(true);
    assert_eq!(
        t.command(
            &t.player,
            json!({"type":"projectToken","projection":projection})
        )
        .await
        .0,
        StatusCode::OK
    );
    let own = t.view(&t.player).await.1;
    assert_eq!(own["vitals"][0]["ac"], 15);
    assert_eq!(own["tokens"][0]["name"], "Chosen projection");
    assert!(t.view(&t.other).await.1["vitals"][0].get("hp").is_none());
    projection["notes"] = json!("No full sheet upload");
    assert_eq!(
        t.command(
            &t.player,
            json!({"type":"projectToken","projection":projection})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn portraits_require_original_bytes_and_cannot_reference_another_seats_assets() {
    let t = TableFixture::new().await;
    let scene = t.scene().await;
    t.activate(&scene).await;
    let token = t.token(&scene, &t.player, false).await;
    let bytes = common::table::png();
    let gm =
        t.f.state
            .database
            .put_map(t.gm["token"].as_str().unwrap(), &t.id, &bytes, "image/png")
            .unwrap();
    let p = json!({"tokenId":token["id"],"characterId":uuid::Uuid::new_v4().to_string(),"name":null,"speed":null,"portraitId":gm.id,"vitals":null});
    assert_eq!(
        t.command(&t.player, json!({"type":"projectToken","projection":p}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let asset =
        t.f.state
            .database
            .put_portrait(
                t.player["token"].as_str().unwrap(),
                &t.id,
                &bytes,
                "image/png",
            )
            .unwrap();
    assert_eq!(asset.id, gm.id);
    assert_eq!(
        t.command(&t.player, json!({"type":"projectToken","projection":p}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        t.f.state
            .database
            .get_map(t.player["token"].as_str().unwrap(), &t.id, &asset.id)
            .unwrap()
            .1,
        bytes
    );
    assert!(
        t.f.state
            .database
            .put_portrait(
                t.spectator["token"].as_str().unwrap(),
                &t.id,
                &bytes,
                "image/png"
            )
            .is_err()
    );
}
#[test]
fn generated_board_and_combat_fields_match_the_serialized_camel_case_contract() {
    let api = serde_json::to_value(chronexus_server::routes::contract()).unwrap();
    for name in ["BoardAction", "CombatAction"] {
        let text = api["components"]["schemas"][name].to_string();
        assert!(!text.contains("token_id"));
        assert!(!text.contains("scene_id"));
        assert!(!text.contains("max_hp"));
    }
}
