mod common;
use axum::http::StatusCode;
use common::table::{TableFixture, png};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn prepared_scenes_hidden_tokens_and_geometry_never_reach_players() {
    let g = TableFixture::new().await;
    let public = g.scene().await;
    let mut secret = g.scene().await;
    secret["name"] = json!("SECRET PREPARATION");
    assert_eq!(g.command(&g.gm,json!({"type":"importScene","scene":secret,"geometry":"{\"line_of_sight\":[[{\"x\":1,\"y\":1},{\"x\":2,\"y\":2}]]}"})).await.0,StatusCode::OK);
    g.token(&public, &g.player, false).await;
    g.token(&public, &g.gm, true).await;
    g.token(&secret, &g.other, false).await;
    g.activate(&public).await;
    for user in [&g.player, &g.other, &g.spectator] {
        let (status, view) = g.view(user).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(view["scenes"].as_array().unwrap().len(), 1);
        assert_eq!(view["tokens"].as_array().unwrap().len(), 1);
        assert!(view.get("geometry").is_none());
        assert!(!view.to_string().contains("SECRET"));
    }
    assert_eq!(g.view(&g.gm).await.1["scenes"].as_array().unwrap().len(), 2);
    assert_eq!(g.view(&g.outsider).await.0, StatusCode::FORBIDDEN);
    for user in [&g.player, &g.other, &g.spectator, &g.outsider] {
        assert_eq!(
            g.command(user, json!({"type":"saveScene","scene":public}))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
}
#[tokio::test]
async fn movement_is_owned_snapped_revisioned_and_charged_without_blocking_over_budget() {
    let g = TableFixture::new().await;
    let mut s = g.scene().await;
    s["grid"]["diagonal"] = json!("alternating");
    s["difficult"] = json!([{"x":3,"y":3}]);
    assert_eq!(
        g.command(&g.gm, json!({"type":"saveScene","scene":s}))
            .await
            .0,
        StatusCode::OK
    );
    g.activate(&s).await;
    let token = g.token(&s, &g.player, false).await;
    let mv = |path| json!({"type":"move","tokenId":token["id"],"path":path});
    for user in [&g.other, &g.spectator, &g.outsider] {
        assert_eq!(
            g.command(user, mv(json!([{"x":2,"y":2}]))).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let (_, view) = g.view(&g.gm).await;
    let op = json!({"operationId":Uuid::now_v7().to_string(),"baseRevision":view["revision"],"command":mv(json!([{"x":2,"y":2},{"x":3,"y":3}]))});
    let (status, ack) = g.operation(&g.player, op.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(g.operation(&g.player, op.clone()).await.1, ack);
    let mut changed = op.clone();
    changed["command"]["path"] = json!([{"x":4,"y":4}]);
    assert_eq!(
        g.operation(&g.player, changed).await.0,
        StatusCode::CONFLICT
    );
    let view = g.view(&g.player).await.1;
    assert_eq!(view["tokens"][0]["spent"], 25.0);
    assert_eq!(view["tokens"][0]["diagonals"], 2);
    let mut stale = op;
    stale["operationId"] = json!(Uuid::now_v7().to_string());
    assert_eq!(g.operation(&g.player, stale).await.0, StatusCode::CONFLICT);
    assert_eq!(
        g.command(&g.player, mv(json!([{"x":4.5,"y":4}]))).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        g.command(&g.player, mv(json!([{"x":9,"y":9}]))).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        g.command(&g.player, mv(json!([{"x":4,"y":4},{"x":5,"y":5}])))
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        g.view(&g.player).await.1["tokens"][0]["spent"]
            .as_f64()
            .unwrap()
            > 30.0
    );
    s["freeMove"] = json!(false);
    assert_eq!(
        g.command(&g.gm, json!({"type":"saveScene","scene":s}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        g.command(&g.player, mv(json!([{"x":6,"y":6}]))).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        g.command(&g.gm, mv(json!([{"x":6,"y":6}]))).await.0,
        StatusCode::OK
    );
}
#[tokio::test]
async fn maps_keep_original_bytes_and_do_not_grant_access_to_prepared_assets() {
    use chronexus_server::db::Database;
    let g = TableFixture::new().await;
    let bytes = png();
    let db = &g.f.state.database;
    let gt = g.gm["token"].as_str().unwrap();
    let pt = g.player["token"].as_str().unwrap();
    let asset = db.put_map(gt, &g.id, &bytes, "image/png").unwrap();
    assert_eq!(db.get_map(gt, &g.id, &asset.id).unwrap().1, bytes);
    assert_eq!(
        db.put_map(gt, &g.id, &bytes, "image/png").unwrap().id,
        asset.id
    );
    assert!(db.put_map(pt, &g.id, &bytes, "image/png").is_err());
    assert!(db.put_map(gt, &g.id, &bytes, "image/jpeg").is_err());
    assert!(db.put_map(gt, &g.id, b"<script>", "image/png").is_err());
    assert!(db.get_map(pt, &g.id, &asset.id).is_err());
    let mut s = g.scene().await;
    s["mapId"] = json!(asset.id);
    assert_eq!(
        g.command(&g.gm, json!({"type":"saveScene","scene":s}))
            .await
            .0,
        StatusCode::OK
    );
    g.activate(&s).await;
    assert_eq!(db.get_map(pt, &g.id, &asset.id).unwrap().1, bytes);
    // Whole-server backup/restore contains table assets without touching journals.
    let path = g.f._directory.path().join("board-backup.sqlite3");
    let source = g.f.state.config.database.clone();
    let report = chronexus_server::backup::create(&source, &path).unwrap();
    assert_eq!(report.attachments, 1);
    let restored = g.f._directory.path().join("restored.sqlite3");
    chronexus_server::backup::restore(&restored, &path).unwrap();
    let restored = Database::open(&restored).unwrap();
    assert!(
        restored.get_map(gt, &g.id, &asset.id).is_err(),
        "restore revokes old sessions"
    );
    restored
        .with(|conn| {
            let original: Vec<u8> = conn.query_row(
                "SELECT original FROM table_map_assets WHERE id=?1",
                [&asset.id],
                |r| r.get(0),
            )?;
            assert_eq!(original, bytes);
            Ok(())
        })
        .unwrap();
    db.with(|conn| {
        conn.execute(
            "UPDATE table_map_assets SET original=x'00' WHERE id=?1",
            [&asset.id],
        )?;
        Ok(())
    })
    .unwrap();
    let bad = g.f._directory.path().join("corrupt-map-backup.sqlite3");
    assert!(chronexus_server::backup::create(&source, &bad).is_err());
    assert!(
        !bad.exists(),
        "a backup with a corrupted original must not be reported as saved"
    );
}
#[tokio::test]
async fn uvtt_import_keeps_occluders_gm_only_and_rejects_remote_images() {
    use base64::Engine;
    let g = TableFixture::new().await;
    let doc = json!({"format":0.3,"resolution":{"map_origin":{"x":0,"y":0},"map_size":{"x":2,"y":2},"pixels_per_grid":50},"image":base64::engine::general_purpose::STANDARD.encode(png()),"line_of_sight":[[{"x":0,"y":0},{"x":1,"y":1}]],"portals":[{"position":{"x":1,"y":1},"bounds":[{"x":0,"y":0},{"x":1,"y":1}],"closed":true}],"lights":[{"position":{"x":1,"y":1},"range":5,"color":"ffffffff","intensity":1,"shadows":true}],"environment":{"ambient_light":"ffffffff"}});
    let (_, view) = g.view(&g.gm).await;
    let input = json!({"operationId":Uuid::now_v7().to_string(),"baseRevision":view["revision"],"sceneId":Uuid::new_v4().to_string(),"name":"UVTT","document":doc});
    let path = format!("/api/v1/tables/{}/uvtt", g.id);
    let (status, body) =
        g.f.call("POST", &path, input.clone(), g.gm["token"].as_str())
            .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        g.f.call("POST", &path, input.clone(), g.gm["token"].as_str())
            .await
            .1,
        body
    );
    let board = g.view(&g.gm).await.1;
    assert!(board["geometry"].to_string().contains("line_of_sight"));
    assert_eq!(g.view(&g.player).await.1["scenes"], json!([]));
    let mut bad = input;
    bad["document"]["image"] = json!("https://example.com/private.png");
    assert_eq!(
        g.f.call("POST", &path, bad, g.gm["token"].as_str()).await.0,
        StatusCode::BAD_REQUEST
    );
}
#[test]
fn distance_conversion_preserves_both_conventions_and_original_values() {
    for value in [0.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 5280.0] {
        for convention in ["table", "exact"] {
            let m = chronexus_server::vtt::convert(value, "ft", "m", convention);
            assert!(
                (chronexus_server::vtt::convert(m, "m", "ft", convention) - value).abs() < 1e-9
            );
        }
    }
    assert_eq!(
        chronexus_server::vtt::convert(30.0, "ft", "m", "table"),
        9.0
    );
    assert!((chronexus_server::vtt::convert(30.0, "ft", "m", "exact") - 9.144).abs() < 1e-9);
}

#[tokio::test]
async fn template_ownership_and_hidden_preparation_are_enforced_on_every_view() {
    let g = TableFixture::new().await;
    let scene = g.scene().await;
    let prepared = g.scene().await;
    g.activate(&scene).await;
    let public = json!({"id":Uuid::new_v4().to_string(),"sceneId":scene["id"],"ownerId":g.player["accountId"],"shape":"cone","origin":{"x":3,"y":3},"angle":45,"size":{"value":15,"unit":"ft"},"coverage":"center","hidden":false});
    assert_eq!(
        g.command(&g.player, json!({"type":"saveTemplate","template":public}))
            .await
            .0,
        StatusCode::OK
    );
    let mut hidden = public.clone();
    hidden["id"] = json!(Uuid::new_v4().to_string());
    hidden["hidden"] = json!(true);
    hidden["ownerId"] = g.gm["accountId"].clone();
    assert_eq!(
        g.command(&g.gm, json!({"type":"saveTemplate","template":hidden}))
            .await
            .0,
        StatusCode::OK
    );
    for user in [&g.player, &g.other, &g.spectator] {
        let view = g.view(user).await.1;
        assert_eq!(view["templates"].as_array().unwrap().len(), 1);
        assert_eq!(view["templates"][0]["id"], public["id"]);
    }
    assert_eq!(
        g.command(
            &g.other,
            json!({"type":"removeTemplate","templateId":public["id"]})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let mut stolen = public.clone();
    stolen["ownerId"] = g.other["accountId"].clone();
    assert_eq!(
        g.command(&g.other, json!({"type":"saveTemplate","template":stolen}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut private = public.clone();
    private["sceneId"] = prepared["id"].clone();
    assert_eq!(
        g.command(&g.player, json!({"type":"saveTemplate","template":private}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        g.command(
            &g.player,
            json!({"type":"removeTemplate","templateId":public["id"]})
        )
        .await
        .0,
        StatusCode::OK
    );
}
