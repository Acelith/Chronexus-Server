use super::Fixture;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;
pub struct TableFixture {
    pub f: Fixture,
    pub gm: Value,
    pub player: Value,
    pub other: Value,
    pub spectator: Value,
    pub outsider: Value,
    pub id: String,
}
impl TableFixture {
    pub async fn new() -> Self {
        let f = Fixture::new();
        let code = f.state.database.issue_invite_with_gm(None, true).unwrap();
        let(status,gm)=f.call("POST","/api/v1/auth/redeem",json!({"inviteCode":code,"username":"board-gm","displayName":"GM","password":"synthetic-password-123","device":"test"}),None).await;
        assert_eq!(status, StatusCode::OK);
        let player = f.account("board-player").await;
        let other = f.account("board-other").await;
        let spectator = f.account("board-spectator").await;
        let outsider = f.account("board-outsider").await;
        let (status, table) = f
            .call(
                "POST",
                "/api/v1/tables",
                json!({"name":"Synthetic board"}),
                gm["token"].as_str(),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        let id = table["id"].as_str().unwrap().to_string();
        for (user, role) in [
            (&player, "player"),
            (&other, "player"),
            (&spectator, "spectator"),
        ] {
            let (_, invite) = f
                .call(
                    "POST",
                    &format!("/api/v1/tables/{id}/invites"),
                    json!({"role":role}),
                    gm["token"].as_str(),
                )
                .await;
            assert_eq!(
                f.call(
                    "POST",
                    "/api/v1/tables/join",
                    json!({"inviteCode":invite["code"]}),
                    user["token"].as_str()
                )
                .await
                .0,
                StatusCode::OK
            );
        }
        Self {
            f,
            gm,
            player,
            other,
            spectator,
            outsider,
            id,
        }
    }
    pub async fn view(&self, user: &Value) -> (StatusCode, Value) {
        self.f
            .call(
                "GET",
                &format!("/api/v1/tables/{}/board", self.id),
                Value::Null,
                user["token"].as_str(),
            )
            .await
    }
    pub async fn command(&self, user: &Value, command: Value) -> (StatusCode, Value) {
        let (_, view) = self.view(&self.gm).await;
        self.operation(user,json!({"operationId":Uuid::now_v7().to_string(),"baseRevision":view["revision"],"command":command})).await
    }
    pub async fn operation(&self, user: &Value, op: Value) -> (StatusCode, Value) {
        self.f
            .call(
                "POST",
                &format!("/api/v1/tables/{}/board/operations", self.id),
                op,
                user["token"].as_str(),
            )
            .await
    }
    pub async fn scene(&self) -> Value {
        let scene = scene();
        let (status, body) = self
            .command(&self.gm, json!({"type":"saveScene","scene":scene}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        scene
    }
    pub async fn token(&self, scene: &Value, owner: &Value, hidden: bool) -> Value {
        let token = json!({"id":Uuid::new_v4().to_string(),"sceneId":scene["id"],"ownerId":owner["accountId"],"name":if hidden{"SECRET MONSTER"}else{"Visible hero"},"position":{"x":1,"y":1},"size":1,"speed":{"value":30,"unit":"ft"},"hidden":hidden,"portraitId":null,"characterId":null,"spent":0,"diagonals":0});
        let (status, body) = self
            .command(&self.gm, json!({"type":"saveToken","token":token}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        token
    }
    pub async fn activate(&self, scene: &Value) {
        assert_eq!(
            self.command(
                &self.gm,
                json!({"type":"activateScene","sceneId":scene["id"]})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
}
pub fn scene() -> Value {
    json!({"id":Uuid::new_v4().to_string(),"name":"Synthetic scene","width":1000,"height":700,"mapId":null,"grid":{"kind":"square","pixels":50,"offset":{"x":0,"y":0},"color":"#cab582","opacity":0.4,"distance":{"value":5,"unit":"ft"},"diagonal":"equal","conversion":"table"},"freeMove":true,"difficult":[]})
}
pub fn png() -> Vec<u8> {
    let mut image = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(100, 100)
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    image.into_inner()
}
