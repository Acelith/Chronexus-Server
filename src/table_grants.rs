//! Explicit per-item journal grants to a table GM; no campaign membership is implied.
use crate::{
    AppState,
    campaigns::{identifier, member},
    db::Database,
    error::{ApiError, ErrorBody, Result},
    routes::{body, token},
};
use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantKey {
    pub campaign_id: String,
    pub kind: String,
    pub item_id: String,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrantedItem {
    pub campaign_id: String,
    pub kind: String,
    pub item_id: String,
    pub author_name: String,
    pub revision: i64,
    pub payload: crate::payload::Payload,
}

impl Database {
    pub fn grant_gm_item(&self, token: &str, table: &str, key: &GrantKey) -> Result<()> {
        identifier(table)?;
        identifier(&key.campaign_id)?;
        identifier(&key.item_id)?;
        if ![
            "page",
            "session",
            "relationship",
            "party",
            "clue",
            "sheetSection",
            "image",
        ]
        .contains(&key.kind.as_str())
        {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            member(&tx,&key.campaign_id,&identity.account_id)?;
            let seat:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM table_seats WHERE table_id=?1 AND account_id=?2 AND role='player')",params![table,identity.account_id],|r|r.get(0))?;
            if !seat { return Err(ApiError(StatusCode::FORBIDDEN,"Player seat required for journal grant.")); }
            let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM content WHERE campaign_id=?1 AND kind=?2 AND id=?3 AND author_id=?4 AND visibility='members' AND deleted=0)",params![key.campaign_id,key.kind,key.item_id,identity.account_id],|r|r.get(0))?;
            if !owned { return Err(ApiError(StatusCode::NOT_FOUND,"Shared item not found.")); }
            tx.execute("INSERT INTO gm_item_grants(table_id,campaign_id,kind,item_id,author_id,granted_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(table_id,campaign_id,kind,item_id) DO UPDATE SET author_id=excluded.author_id,granted_at=excluded.granted_at",params![table,key.campaign_id,key.kind,key.item_id,identity.account_id,crate::auth::now()])?;
            tx.commit()?; Ok(())
        })
    }
    pub fn revoke_gm_item(&self, token: &str, table: &str, key: &GrantKey) -> Result<()> {
        identifier(table)?;
        identifier(&key.campaign_id)?;
        identifier(&key.item_id)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            member(conn,&key.campaign_id,&identity.account_id)?;
            let count=conn.execute("DELETE FROM gm_item_grants WHERE table_id=?1 AND campaign_id=?2 AND kind=?3 AND item_id=?4 AND author_id=?5",params![table,key.campaign_id,key.kind,key.item_id,identity.account_id])?;
            if count==0 { return Err(ApiError(StatusCode::NOT_FOUND,"Journal grant not found.")); }
            Ok(())
        })
    }
    pub fn gm_items(&self, token: &str, table: &str) -> Result<Vec<GrantedItem>> {
        identifier(table)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let role:Option<String>=conn.query_row("SELECT role FROM table_seats WHERE table_id=?1 AND account_id=?2",params![table,identity.account_id],|r|r.get(0)).optional()?;
            if role.as_deref()!=Some("gm") { return Err(ApiError(StatusCode::FORBIDDEN,"GM seat required.")); }
            let mut query=conn.prepare("SELECT g.campaign_id,g.kind,g.item_id,a.display_name,c.revision,c.payload FROM gm_item_grants g JOIN content c ON c.campaign_id=g.campaign_id AND c.kind=g.kind AND c.id=g.item_id AND c.author_id=g.author_id JOIN accounts a ON a.id=g.author_id JOIN members m ON m.campaign_id=g.campaign_id AND m.account_id=g.author_id JOIN table_seats s ON s.table_id=g.table_id AND s.account_id=g.author_id AND s.role='player' WHERE g.table_id=?1 AND c.visibility='members' AND c.deleted=0 AND m.active=1 AND a.disabled=0 ORDER BY g.granted_at,g.item_id LIMIT 100")?;
            let rows=query.query_map([table],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
            rows.into_iter().map(|(campaign_id,kind,item_id,author_name,revision,payload)|Ok(GrantedItem{campaign_id,kind,item_id,author_name,revision,payload:serde_json::from_str(&payload).map_err(|_|ApiError::internal())?})).collect()
        })
    }
}
pub(crate) fn router() -> Router<AppState> {
    Router::new().route(
        "/api/v1/tables/{tableId}/journal-grants",
        post(grant).get(list).delete(revoke),
    )
}
#[utoipa::path(post,path="/api/v1/tables/{tableId}/journal-grants",params(("tableId"=String,Path)),request_body=GrantKey,security(("bearer"=[])),responses((status=204),(status=403,body=ErrorBody)),tag="tables")]
async fn grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    input: std::result::Result<Json<GrantKey>, JsonRejection>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.grant_gm_item(&token, &id, &input))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(delete,path="/api/v1/tables/{tableId}/journal-grants",params(("tableId"=String,Path)),request_body=GrantKey,security(("bearer"=[])),responses((status=204),(status=403,body=ErrorBody)),tag="tables")]
async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    input: std::result::Result<Json<GrantKey>, JsonRejection>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.revoke_gm_item(&token, &id, &input))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get,path="/api/v1/tables/{tableId}/journal-grants",params(("tableId"=String,Path)),security(("bearer"=[])),responses((status=200,body=Vec<GrantedItem>),(status=403,body=ErrorBody)),tag="tables")]
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<GrantedItem>>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.gm_items(&token, &id))
        .await
        .map(Json)
}
#[derive(OpenApi)]
#[openapi(
    paths(grant, revoke, list),
    components(schemas(GrantKey, GrantedItem, ErrorBody))
)]
pub(crate) struct GrantContract;
