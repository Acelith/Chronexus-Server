mod common;
use axum::http::StatusCode;
use common::table::TableFixture;
use serde_json::{Value, json};
use uuid::Uuid;
fn combat(action: Value) -> Value {
    json!({"type":"combat","action":action})
}
async fn ok(g: &TableFixture, user: &Value, action: Value) {
    let (status, body) = g.command(user, combat(action)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
fn vitals(token: &Value, hp: i32, show: bool, note: Option<&str>) -> Value {
    json!({"type":"setVitals","tokenId":token["id"],"hp":hp,"maxHp":30,"ac":15,"showHp":show,"gmNote":note})
}
fn token_vitals(view: &Value, id: &Value) -> Value {
    view["vitals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["tokenId"] == *id)
        .unwrap()
        .clone()
}
#[tokio::test]
async fn initiative_ties_turns_three_rounds_and_expiry_require_explicit_actions() {
    let g = TableFixture::new().await;
    let s = g.scene().await;
    g.activate(&s).await;
    let a = g.token(&s, &g.player, false).await;
    let b = g.token(&s, &g.other, false).await;
    let m = g.token(&s, &g.gm, false).await;
    ok(
        &g,
        &g.gm,
        json!({"type":"start","sceneId":s["id"],"tokenIds":[a["id"],b["id"],m["id"]]}),
    )
    .await;
    for (owner, t, value) in [(&g.player, &a, 15), (&g.other, &b, 15), (&g.gm, &m, 10)] {
        ok(&g,owner,json!({"type":"initiative","tokenId":t["id"],"value":value,"roll":false,"secret":false})).await;
    }
    ok(
        &g,
        &g.gm,
        json!({"type":"order","tokenIds":[b["id"],a["id"],m["id"]]}),
    )
    .await;
    let condition = Uuid::new_v4().to_string();
    ok(&g,&g.player,json!({"type":"setCondition","condition":{"id":condition,"tokenId":a["id"],"name":"poisoned","rounds":1,"expired":false}})).await;
    assert_eq!(g.view(&g.player).await.1["combat"]["round"], 0);
    ok(&g, &g.gm, json!({"type":"next"})).await;
    assert_eq!(g.view(&g.player).await.1["combat"]["current"], b["id"]);
    let movement = |t: &Value| json!({"type":"move","tokenId":t["id"],"path":[{"x":2,"y":1}]});
    assert_eq!(
        g.command(&g.player, movement(&a)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(g.command(&g.other, movement(&b)).await.0, StatusCode::OK);
    for _ in 0..3 {
        ok(&g, &g.gm, json!({"type":"next"})).await;
    }
    let view = g.view(&g.player).await.1;
    assert_eq!(view["combat"]["round"], 2);
    assert_eq!(view["combat"]["conditions"][0]["rounds"], 0);
    assert_eq!(view["combat"]["conditions"][0]["expired"], true);
    let moved = g.view(&g.other).await.1;
    assert_eq!(
        moved["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == b["id"])
            .unwrap()["spent"],
        0.0
    );
    for _ in 0..3 {
        ok(&g, &g.gm, json!({"type":"next"})).await;
    }
    assert_eq!(g.view(&g.player).await.1["combat"]["round"], 3);
    assert_eq!(
        g.view(&g.player).await.1["combat"]["conditions"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "expired conditions are retained"
    );
    ok(
        &g,
        &g.player,
        json!({"type":"removeCondition","conditionId":condition}),
    )
    .await;
    ok(&g, &g.gm, json!({"type":"delay"})).await;
    assert_eq!(g.view(&g.player).await.1["combat"]["current"], a["id"]);
    ok(&g, &g.gm, json!({"type":"remove","tokenId":a["id"]})).await;
    ok(&g, &g.gm, json!({"type":"add","tokenId":a["id"]})).await;
    let duplicate = json!({"type":"order","tokenIds":[b["id"],b["id"],a["id"]]});
    assert_eq!(
        g.command(&g.gm, combat(duplicate)).await.0,
        StatusCode::BAD_REQUEST
    );
    ok(&g, &g.gm, json!({"type":"end"})).await;
    assert_eq!(g.command(&g.player, movement(&a)).await.0, StatusCode::OK);
}
#[tokio::test]
async fn hp_and_armour_are_owner_only_even_against_the_gm_and_private_logs_stay_private() {
    let g = TableFixture::new().await;
    let s = g.scene().await;
    g.activate(&s).await;
    let p = g.token(&s, &g.player, false).await;
    let m = g.token(&s, &g.gm, false).await;
    ok(&g, &g.player, vitals(&p, 27, false, None)).await;
    let mut monster_stats = vitals(&m, 19, false, Some("SECRET MONSTER TACTICS"));
    monster_stats["status"] = json!("Bloodied");
    ok(&g, &g.gm, monster_stats).await;
    for user in [&g.other, &g.gm, &g.spectator] {
        let v = g.view(user).await.1;
        let actual = token_vitals(&v, &p["id"]);
        assert!(actual.get("hp").is_none());
        assert!(actual.get("ac").is_none());
        assert!(
            !v["combatLog"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["tokenId"] == p["id"] && v["kind"] == "hp")
        );
    }
    for user in [&g.gm, &g.other, &g.spectator, &g.outsider] {
        assert_eq!(
            g.command(user, combat(vitals(&p, 1, true, None))).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let mut stolen = p.clone();
    stolen["ownerId"] = g.gm["accountId"].clone();
    assert_eq!(
        g.command(&g.gm, json!({"type":"saveToken","token":stolen}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (_, view) = g.view(&g.gm).await;
    let op = json!({"operationId":Uuid::now_v7().to_string(),"baseRevision":view["revision"],"command":combat(vitals(&p,20,true,None))});
    let (status, ack) = g.operation(&g.player, op.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(g.operation(&g.player, op).await.1, ack);
    assert_eq!(token_vitals(&g.view(&g.gm).await.1, &p["id"])["hp"], 20);
    let view = g.view(&g.player).await.1;
    assert!(!view.to_string().contains("SECRET"));
    assert!(token_vitals(&view, &m["id"]).get("hp").is_none());
    assert_eq!(token_vitals(&view, &m["id"])["status"], "Bloodied");
    assert_eq!(
        token_vitals(&g.view(&g.gm).await.1, &m["id"])["gmNote"],
        "SECRET MONSTER TACTICS"
    );
}
#[tokio::test]
async fn secret_rolls_hidden_turns_conditions_and_prepared_combat_never_reach_other_seats() {
    let g = TableFixture::new().await;
    let s = g.scene().await;
    g.activate(&s).await;
    let p = g.token(&s, &g.player, false).await;
    let hidden = g.token(&s, &g.gm, true).await;
    let visible = g.token(&s, &g.gm, false).await;
    ok(&g,&g.gm,json!({"type":"roll","tokenId":visible["id"],"count":2,"sides":20,"modifier":3,"secret":true,"label":"SECRET ROLL"})).await;
    ok(
        &g,
        &g.gm,
        json!({"type":"start","sceneId":s["id"],"tokenIds":[hidden["id"],p["id"]]}),
    )
    .await;
    ok(
        &g,
        &g.gm,
        json!({"type":"initiative","tokenId":hidden["id"],"value":20,"roll":true,"secret":true}),
    )
    .await;
    ok(&g,&g.gm,json!({"type":"setCondition","condition":{"id":Uuid::new_v4().to_string(),"tokenId":hidden["id"],"name":"invisible","rounds":5,"expired":false}})).await;
    ok(&g, &g.gm, json!({"type":"next"})).await;
    for user in [&g.player, &g.other, &g.spectator] {
        let view = g.view(user).await.1;
        assert!(!view.to_string().contains("SECRET"));
        assert!(!view.to_string().contains(hidden["id"].as_str().unwrap()));
        assert_eq!(view["combat"]["current"], Value::Null);
        assert_eq!(view["combat"]["order"].as_array().unwrap().len(), 1);
    }
    let view = g.view(&g.gm).await.1;
    let roll = view["combatLog"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == "SECRET ROLL")
        .unwrap();
    let dice = roll["dice"].as_array().unwrap();
    assert_eq!(dice.len(), 2);
    for d in dice {
        assert!((1..=20).contains(&d.as_u64().unwrap()));
    }
    assert_eq!(
        roll["amount"].as_u64().unwrap(),
        dice.iter().map(|v| v.as_u64().unwrap()).sum::<u64>() + 3
    );
    assert_eq!(g.command(&g.player,combat(json!({"type":"roll","tokenId":p["id"],"count":1,"sides":20,"modifier":0,"secret":true,"label":"Invalid"}))).await.0,StatusCode::FORBIDDEN);
    assert_eq!(g.command(&g.player,combat(json!({"type":"roll","tokenId":p["id"],"count":999,"sides":20,"modifier":0,"secret":false,"label":"Invalid"}))).await.0,StatusCode::BAD_REQUEST);
    ok(&g, &g.gm, json!({"type":"end"})).await;
    let prepared = g.scene().await;
    let monster = g.token(&prepared, &g.gm, false).await;
    ok(
        &g,
        &g.gm,
        json!({"type":"start","sceneId":prepared["id"],"tokenIds":[monster["id"]]}),
    )
    .await;
    assert_eq!(g.view(&g.player).await.1["combat"], Value::Null);
    let playerlog = g.view(&g.player).await.1["combatLog"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["kind"] == "start")
        .count();
    assert_eq!(playerlog, 1, "preparation-scene combat is excluded");
}
#[tokio::test]
async fn removing_tokens_and_restoring_combat_do_not_leave_live_turns_or_reusable_sessions() {
    let g = TableFixture::new().await;
    let s = g.scene().await;
    g.activate(&s).await;
    let a = g.token(&s, &g.player, false).await;
    let b = g.token(&s, &g.gm, false).await;
    ok(&g, &g.player, vitals(&a, 25, false, None)).await;
    ok(
        &g,
        &g.gm,
        json!({"type":"start","sceneId":s["id"],"tokenIds":[a["id"],b["id"]]}),
    )
    .await;
    ok(&g, &g.gm, json!({"type":"next"})).await;
    assert_eq!(
        g.command(&g.gm, json!({"type":"removeToken","tokenId":a["id"]}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(g.view(&g.gm).await.1["combat"]["current"], b["id"]);
    let backup = g.f._directory.path().join("combat-backup.sqlite3");
    chronexus_server::backup::create(&g.f.state.config.database, &backup).unwrap();
    let restored = g.f._directory.path().join("combat-restored.sqlite3");
    chronexus_server::backup::restore(&restored, &backup).unwrap();
    let db = chronexus_server::db::Database::open(&restored).unwrap();
    assert!(
        db.board_view(g.gm["token"].as_str().unwrap(), &g.id)
            .is_err()
    );
    db.with(|conn| {
        let data: String = conn.query_row(
            "SELECT data FROM table_combats WHERE table_id=?1",
            [&g.id],
            |r| r.get(0),
        )?;
        let data: Value = serde_json::from_str(&data).unwrap();
        assert_eq!(data["current"], b["id"]);
        Ok(())
    })
    .unwrap();
}
