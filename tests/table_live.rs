mod common;
use axum::http::StatusCode;
use common::table::TableFixture;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use uuid::Uuid;
#[tokio::test]
async fn live_snapshots_filter_secrets_reconnect_and_reject_removed_seats() {
    let g = TableFixture::new().await;
    let s = g.scene().await;
    g.activate(&s).await;
    let token = g.token(&s, &g.player, false).await;
    g.token(&s, &g.gm, true).await;
    let guard = g.token(&s, &g.gm, false).await;
    for action in [
        json!({"type":"setVitals","tokenId":guard["id"],"hp":19,"maxHp":30,"ac":15,"showHp":false,"gmNote":"SECRET GUARD NOTE"}),
        json!({"type":"roll","tokenId":guard["id"],"count":1,"sides":20,"modifier":0,"secret":true,"label":"SECRET GUARD ROLL"}),
    ] {
        let (status, body) = g
            .command(&g.gm, json!({"type":"combat","action":action}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = g.f.app.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let path = format!("/api/v1/tables/{}/live-ticket", g.id);
    let (_, ticket) =
        g.f.call("POST", &path, Value::Null, g.player["token"].as_str())
            .await;
    let url = format!(
        "ws://127.0.0.1:{port}/api/v1/tables/{}/live?ticket={}",
        g.id,
        ticket["ticket"].as_str().unwrap()
    );
    let mut request = url.clone().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("host", "127.0.0.1:8080".parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let frame = socket.next().await.unwrap().unwrap().into_text().unwrap();
    assert!(!frame.contains("SECRET"));
    let state: Value = serde_json::from_str(&frame).unwrap();
    assert_eq!(state["board"]["tokens"].as_array().unwrap().len(), 2);
    let op = json!({"operationId":Uuid::now_v7().to_string(),"baseRevision":state["revision"],"command":{"type":"move","tokenId":token["id"],"path":[{"x":2,"y":1}]}});
    let started = std::time::Instant::now();
    socket
        .send(Message::Text(op.to_string().into()))
        .await
        .unwrap();
    let mut received = false;
    for _ in 0..5 {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        assert!(!frame.contains("SECRET"));
        let frame: Value = serde_json::from_str(&frame).unwrap();
        if frame["type"] == "state"
            && frame["board"]["tokens"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == token["id"])
                .unwrap()["position"]["x"]
                == 2.0
        {
            received = true;
            break;
        }
    }
    assert!(received);
    println!(
        "Loopback move acknowledgment and filtered state: {} ms",
        started.elapsed().as_millis()
    );
    socket.close(None).await.unwrap();
    let (_, next) =
        g.f.call("POST", &path, Value::Null, g.player["token"].as_str())
            .await;
    let mut request = format!(
        "ws://127.0.0.1:{port}/api/v1/tables/{}/live?ticket={}",
        g.id,
        next["ticket"].as_str().unwrap()
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("host", "127.0.0.1:8080".parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let frame: Value = serde_json::from_str(
        socket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap()
            .as_str(),
    )
    .unwrap();
    assert_eq!(
        frame["board"]["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == token["id"])
            .unwrap()["position"]["x"],
        2.0
    );
    assert_eq!(
        g.f.call(
            "DELETE",
            &format!(
                "/api/v1/tables/{}/seats/{}",
                g.id,
                g.player["accountId"].as_str().unwrap()
            ),
            Value::Null,
            g.gm["token"].as_str()
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let message = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap();
    assert!(message.is_none_or(|v| v.is_err() || v.is_ok_and(|m| m.is_close())));
    let mut reused = url.into_client_request().unwrap();
    reused
        .headers_mut()
        .insert("host", "127.0.0.1:8080".parse().unwrap());
    assert!(connect_async(reused).await.is_err());
    server.abort();
}
