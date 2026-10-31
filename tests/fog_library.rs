mod common;
use axum::http::StatusCode;
use common::table::TableFixture;
use serde_json::{Value, json};
use uuid::Uuid;
#[tokio::test]
async fn uvtt_vision_conversion_is_explicit_bounded_and_does_not_enable_fog() {
    let t = TableFixture::new().await;
    let s = t.scene().await;
    let geometry = json!({"line_of_sight":[[{"x":1,"y":1},{"x":2,"y":2}]],"portals":[{"position":{"x":2,"y":2},"bounds":[{"x":2,"y":2},{"x":3,"y":2}],"closed":true}],"lights":[{"position":{"x":1,"y":1},"range":10,"color":"#ffffff","intensity":1,"shadows":true}]});
    assert_eq!(
        t.command(
            &t.gm,
            json!({"type":"importScene","scene":s,"geometry":geometry.to_string()})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        t.command(&t.gm, json!({"type":"importVision","sceneId":s["id"]}))
            .await
            .0,
        StatusCode::OK
    );
    let view = t.view(&t.gm).await.1;
    let f = &view["fogEditor"][0];
    assert_eq!(f["enabled"], false);
    assert_eq!(f["vision"], false);
    assert_eq!(f["walls"].as_array().unwrap().len(), 2);
    assert_eq!(f["walls"][0]["a"]["x"], 50.0);
    assert_eq!(f["lights"][0]["range"]["value"], 50.0);
}
fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Value {
    json!({"reveal":true,"points":[{"x":x,"y":y},{"x":x+w,"y":y},{"x":x+w,"y":y+h},{"x":x,"y":y+h}]})
}
#[tokio::test]
async fn catalog_is_gm_only_and_rejects_invalid_requests_without_external_calls() {
    let t = TableFixture::new().await;
    let path = format!("/api/v1/tables/{}/catalog", t.id);
    for user in [&t.player, &t.other, &t.spectator, &t.outsider] {
        assert_eq!(
            t.f.call(
                "POST",
                &path,
                json!({"kind":"monsters","query":"goblin","entry":null}),
                user["token"].as_str()
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    for body in [
        json!({"kind":"other","query":"","entry":null}),
        json!({"kind":"monsters","query":"","entry":"../../private"}),
        json!({"kind":"assets","query":"","entry":"https://127.0.0.1/private"}),
        json!({"kind":"assets","query":"x".repeat(101),"entry":null}),
    ] {
        assert_eq!(
            t.f.call("POST", &path, body, t.gm["token"].as_str())
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
}
#[tokio::test]
async fn fog_filters_http_snapshots_tokens_combat_and_original_map_downloads() {
    let t = TableFixture::new().await;
    let mut s = t.scene().await;
    let mut image = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        100,
        100,
        image::Rgba([255, 0, 0, 255]),
    ))
    .write_to(&mut image, image::ImageFormat::Png)
    .unwrap();
    let map =
        t.f.state
            .database
            .put_map(
                t.gm["token"].as_str().unwrap(),
                &t.id,
                image.get_ref(),
                "image/png",
            )
            .unwrap();
    s["mapId"] = json!(map.id);
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveScene","scene":s}))
            .await
            .0,
        StatusCode::OK
    );
    t.token(&s, &t.player, false).await;
    t.activate(&s).await;
    let fog = json!({"sceneId":s["id"],"enabled":true,"strokes":[]});
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveFog","fog":fog}))
            .await
            .0,
        StatusCode::OK
    );
    for user in [&t.player, &t.other, &t.spectator] {
        let (_, v) = t.view(user).await;
        assert_eq!(v["tokens"].as_array().unwrap().len(), 0);
        assert!(v.get("fogEditor").is_none());
        assert!(v.get("monsters").is_none());
        assert!(v["fog"][0]["visible"].as_array().unwrap().is_empty());
    }
    let (_, bytes) =
        t.f.state
            .database
            .get_map(t.player["token"].as_str().unwrap(), &t.id, &map.id)
            .unwrap();
    assert_eq!(
        image::load_from_memory(&bytes)
            .unwrap()
            .into_rgba8()
            .get_pixel(5, 5)
            .0,
        [0, 0, 0, 255]
    );
    {
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let request = axum::http::Request::builder()
            .uri(format!("/api/v1/tables/{}/maps/{}/thumbnail", t.id, map.id))
            .header("host", "127.0.0.1:8080")
            .header(
                "authorization",
                format!("Bearer {}", t.player["token"].as_str().unwrap()),
            )
            .extension(axum::extract::ConnectInfo(
                "127.0.0.1:50000".parse::<std::net::SocketAddr>().unwrap(),
            ))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = t.f.app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let thumb =
            image::load_from_memory(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap()
                .into_rgba8();
        assert!(thumb.width() <= 128 && thumb.height() <= 128);
        assert_eq!(thumb.get_pixel(5, 5).0, [0, 0, 0, 255]);
    }
    assert_eq!(
        t.f.state
            .database
            .get_map(t.gm["token"].as_str().unwrap(), &t.id, &map.id)
            .unwrap()
            .1,
        *image.get_ref()
    );
    assert_eq!(t.command(&t.gm,json!({"type":"saveFog","fog":{"sceneId":s["id"],"enabled":true,"strokes":[rectangle(0.,0.,200.,200.)]}})).await.0,StatusCode::OK);
    assert_eq!(
        t.view(&t.player).await.1["tokens"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (_, bytes) =
        t.f.state
            .database
            .get_map(t.player["token"].as_str().unwrap(), &t.id, &map.id)
            .unwrap();
    let img = image::load_from_memory(&bytes).unwrap().into_rgba8();
    assert_eq!(img.get_pixel(5, 5).0, [255, 0, 0, 255]);
    assert_eq!(img.get_pixel(90, 90).0, [0, 0, 0, 255]);
}
#[tokio::test]
async fn vision_is_per_owner_and_closed_walls_occlude_until_door_is_opened() {
    let t = TableFixture::new().await;
    let s = t.scene().await;
    let hero = t.token(&s, &t.player, false).await;
    t.activate(&s).await;
    let mut fog = json!({"sceneId":s["id"],"enabled":true,"vision":true,"strokes":[rectangle(0.,0.,1000.,700.)],"eyes":[{"tokenId":hero["id"],"range":{"value":120,"unit":"ft"},"darkvision":true}],"walls":[{"a":{"x":300,"y":0},"b":{"x":300,"y":700},"door":true,"open":false}],"lights":[]});
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveFog","fog":fog}))
            .await
            .0,
        StatusCode::OK
    );
    let (_, v) = t.view(&t.player).await;
    let mask = v["fog"][0]["visible"].as_array().unwrap();
    assert!(!mask.is_empty());
    assert!(mask.iter().all(|v| v.as_u64().unwrap() % 32 < 10));
    assert!(
        t.view(&t.other).await.1["fog"][0]["visible"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fog["walls"][0]["open"] = json!(true);
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveFog","fog":fog}))
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        t.view(&t.player).await.1["fog"][0]["visible"]
            .as_array()
            .unwrap()
            .len()
            > mask.len()
    );
    fog["eyes"][0]["darkvision"] = json!(false);
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveFog","fog":fog}))
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        t.view(&t.player).await.1["fog"][0]["visible"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fog["lights"] = json!([{"position":{"x":50,"y":50},"range":{"value":120,"unit":"ft"}}]);
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveFog","fog":fog}))
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        !t.view(&t.player).await.1["fog"][0]["visible"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn players_cannot_change_fog_library_or_move_through_unrevealed_areas() {
    let t = TableFixture::new().await;
    let s = t.scene().await;
    let hero = t.token(&s, &t.player, false).await;
    t.activate(&s).await;
    let command = json!({"type":"saveFog","fog":{"sceneId":s["id"],"enabled":true,"strokes":[]}});
    for user in [&t.player, &t.other, &t.spectator] {
        assert_eq!(
            t.command(user, command.clone()).await.0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(t.command(&t.gm, command).await.0, StatusCode::OK);
    assert_eq!(
        t.command(
            &t.player,
            json!({"type":"move","tokenId":hero["id"],"path":[{"x":2,"y":1}]})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(t.command(&t.gm,json!({"type":"saveFog","fog":{"sceneId":s["id"],"enabled":true,"strokes":[{"reveal":true,"points":[]}]}})).await.0,StatusCode::BAD_REQUEST);
}
#[tokio::test]
async fn encounter_deployment_is_atomic_idempotent_and_keeps_monster_originals_private() {
    let t = TableFixture::new().await;
    let s = t.scene().await;
    t.activate(&s).await;
    let mid = Uuid::new_v4().to_string();
    let eid = Uuid::new_v4().to_string();
    let mut m = json!({"id":mid,"name":"Synthetic sentinel","hp":12,"ac":14,"speed":{"value":30,"unit":"ft"},"size":1,"statBlock":"Synthetic action","portraitId":null,"source":{"url":"https://example.org/srd","author":"Synthetic author","license":"CC0","edition":"Synthetic","original":"Immutable source"}});
    assert_eq!(
        t.command(&t.player, json!({"type":"saveMonster","monster":m}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveMonster","monster":m}))
            .await
            .0,
        StatusCode::OK
    );
    m["source"]["original"] = json!("Changed source");
    assert_eq!(
        t.command(&t.gm, json!({"type":"saveMonster","monster":m}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(t.command(&t.gm,json!({"type":"saveEncounter","encounter":{"id":eid,"name":"Synthetic encounter","members":[{"monsterId":mid,"position":{"x":2,"y":2},"hidden":true},{"monsterId":mid,"position":{"x":3,"y":2},"hidden":false}]}})).await.0,StatusCode::OK);
    let op = json!({"operationId":Uuid::now_v7().to_string(),"baseRevision":t.view(&t.gm).await.1["revision"],"command":{"type":"deployEncounter","sceneId":s["id"],"encounterId":eid}});
    assert_eq!(t.operation(&t.gm, op.clone()).await.0, StatusCode::OK);
    assert_eq!(t.operation(&t.gm, op).await.0, StatusCode::OK);
    let (_, gm) = t.view(&t.gm).await;
    assert_eq!(gm["tokens"].as_array().unwrap().len(), 2);
    assert_eq!(gm["vitals"][0]["hp"], 12);
    let (_, player) = t.view(&t.player).await;
    assert_eq!(player["tokens"].as_array().unwrap().len(), 1);
    assert!(player.get("monsters").is_none());
    assert!(player.get("encounters").is_none());
    assert!(
        !serde_json::to_string(&player)
            .unwrap()
            .contains("Synthetic action")
    );
    assert_eq!(
        t.command(
            &t.gm,
            json!({"type":"removeLibrary","kind":"monster","id":mid})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let backup = t.f._directory.path().join("table-backup.sqlite3");
    chronexus_server::backup::create(&t.f.state.config.database, &backup).unwrap();
    assert_eq!(
        chronexus_server::backup::verify(&backup).unwrap().schema,
        14
    );
    let conn =
        rusqlite::Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let original: String = conn
        .query_row(
            "SELECT json_extract(data,'$.source.original') FROM table_library WHERE id=?1",
            [mid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(original, "Immutable source");
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM table_tokens WHERE table_id=?1",
            [&t.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
}
