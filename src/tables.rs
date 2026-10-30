//! Table membership is separate from campaign membership and grants no journal access.
use crate::{AppState, auth::{digest, now, random_secret}, campaigns::{identifier, text}, db::Database, error::{ApiError, ErrorBody, Result}, routes::{body, token}};
use axum::{Json, Router, extract::{Path, State, rejection::JsonRejection}, http::{HeaderMap, StatusCode}, routing::{get, post}};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTable { pub name: String }
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Table { pub id: String, pub name: String, pub role: String, pub revision: i64 }
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InviteRequest { pub role: String }
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TableInvite { pub code: String, pub role: String, pub expires_at: String }
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinTable { pub invite_code: String }

fn table_access(conn: &rusqlite::Connection, table: &str, account: &str) -> Result<String> {
    identifier(table)?;
    conn.query_row("SELECT s.role FROM table_seats s JOIN accounts a ON a.id=s.account_id WHERE s.table_id=?1 AND s.account_id=?2 AND a.disabled=0", params![table,account], |r| r.get(0))
        .optional()?.ok_or(ApiError(StatusCode::FORBIDDEN, "Table access denied."))
}
impl Database {
    pub fn create_table(&self, token: &str, input: &CreateTable) -> Result<Table> {
        text(&input.name, 1, 200)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let count: i64 = tx.query_row("SELECT count(*) FROM game_tables WHERE gm_id=?1", [&identity.account_id], |r| r.get(0))?;
            if count >= 100 { return Err(ApiError::limited()); }
            let id = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO game_tables(id,name,gm_id,created_at) VALUES(?1,?2,?3,?4)", params![id,input.name,identity.account_id,now()])?;
            tx.execute("INSERT INTO table_seats(table_id,account_id,role,joined_at) VALUES(?1,?2,'gm',?3)", params![id,identity.account_id,now()])?;
            tx.commit()?;
            Ok(Table { id, name: input.name.clone(), role: "gm".into(), revision: 1 })
        })
    }
    pub fn tables(&self, token: &str) -> Result<Vec<Table>> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            let mut query = conn.prepare("SELECT t.id,t.name,s.role,t.revision FROM game_tables t JOIN table_seats s ON s.table_id=t.id WHERE s.account_id=?1 ORDER BY t.created_at,t.id LIMIT 200")?;
            Ok(query.query_map([identity.account_id], |r| Ok(Table { id:r.get(0)?, name:r.get(1)?, role:r.get(2)?, revision:r.get(3)? }))?.collect::<std::result::Result<Vec<_>,_>>()?)
        })
    }
    pub fn table(&self, token: &str, id: &str) -> Result<Table> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            let role = table_access(conn,id,&identity.account_id)?;
            conn.query_row("SELECT id,name,revision FROM game_tables WHERE id=?1", [id], |r| Ok(Table { id:r.get(0)?,name:r.get(1)?,role,revision:r.get(2)? })).map_err(Into::into)
        })
    }
    pub fn table_invite(&self, token: &str, id: &str, input: &InviteRequest) -> Result<TableInvite> {
        if !["player", "spectator"].contains(&input.role.as_str()) { return Err(ApiError::invalid()); }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if table_access(&tx,id,&identity.account_id)? != "gm" { return Err(ApiError(StatusCode::FORBIDDEN,"Only the GM may invite table members.")); }
            let code = random_secret()?;
            let expires = now()+7*86400;
            tx.execute("INSERT INTO table_invites(code_hash,table_id,role,created_by,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6)", params![digest(&code),id,input.role,identity.account_id,now(),expires])?;
            tx.commit()?;
            Ok(TableInvite { code, role:input.role.clone(), expires_at:crate::campaigns::timestamp(expires) })
        })
    }
    pub fn join_table(&self, token: &str, input: &JoinTable) -> Result<Table> {
        text(&input.invite_code, 16, 256)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let hash = digest(&input.invite_code);
            let invitation: Option<(String,String)> = tx.query_row("SELECT i.table_id,i.role FROM table_invites i JOIN game_tables t ON t.id=i.table_id JOIN table_seats s ON s.table_id=t.id AND s.account_id=i.created_by AND s.role='gm' JOIN accounts a ON a.id=i.created_by WHERE i.code_hash=?1 AND i.used_at IS NULL AND i.expires_at>?2 AND a.disabled=0",params![hash,now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (id,role) = invitation.ok_or_else(ApiError::gone)?;
            let occupied: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM table_seats WHERE table_id=?1 AND account_id=?2)",params![id,identity.account_id],|r|r.get(0))?;
            if occupied { return Err(ApiError(StatusCode::CONFLICT,"Already seated at this table.")); }
            let count: i64 = tx.query_row("SELECT count(*) FROM table_seats WHERE table_id=?1",[&id],|r|r.get(0))?;
            if count >= 100 { return Err(ApiError::limited()); }
            tx.execute("INSERT INTO table_seats(table_id,account_id,role,joined_at) VALUES(?1,?2,?3,?4)",params![id,identity.account_id,role,now()])?;
            tx.execute("UPDATE table_invites SET used_at=?1,used_by=?2 WHERE code_hash=?3",params![now(),identity.account_id,hash])?;
            let table = tx.query_row("SELECT id,name,revision FROM game_tables WHERE id=?1",[id],|r|Ok(Table{id:r.get(0)?,name:r.get(1)?,role,revision:r.get(2)?}))?;
            tx.commit()?;
            Ok(table)
        })
    }
}
pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/v1/tables",post(create).get(list)).route("/api/v1/tables/join",post(join)).route("/api/v1/tables/{tableId}",get(detail)).route("/api/v1/tables/{tableId}/invites",post(invite))
}
#[utoipa::path(post,path="/api/v1/tables",request_body=CreateTable,security(("bearer"=[])),responses((status=201,body=Table),(status=401,body=ErrorBody)),tag="tables")]
async fn create(State(state):State<AppState>,headers:HeaderMap,input:std::result::Result<Json<CreateTable>,JsonRejection>)->Result<(StatusCode,Json<Table>)>{let token=token(&headers)?;let input=body(input)?;Ok((StatusCode::CREATED,Json(state.blocking(move|db|db.create_table(&token,&input)).await?)))}
#[utoipa::path(get,path="/api/v1/tables",security(("bearer"=[])),responses((status=200,body=Vec<Table>),(status=401,body=ErrorBody)),tag="tables")]
async fn list(State(state):State<AppState>,headers:HeaderMap)->Result<Json<Vec<Table>>>{let token=token(&headers)?;state.blocking(move|db|db.tables(&token)).await.map(Json)}
#[utoipa::path(get,path="/api/v1/tables/{tableId}",params(("tableId"=String,Path)),security(("bearer"=[])),responses((status=200,body=Table),(status=403,body=ErrorBody)),tag="tables")]
async fn detail(State(state):State<AppState>,headers:HeaderMap,Path(id):Path<String>)->Result<Json<Table>>{let token=token(&headers)?;state.blocking(move|db|db.table(&token,&id)).await.map(Json)}
#[utoipa::path(post,path="/api/v1/tables/{tableId}/invites",params(("tableId"=String,Path)),request_body=InviteRequest,security(("bearer"=[])),responses((status=201,body=TableInvite),(status=403,body=ErrorBody)),tag="tables")]
async fn invite(State(state):State<AppState>,headers:HeaderMap,Path(id):Path<String>,input:std::result::Result<Json<InviteRequest>,JsonRejection>)->Result<(StatusCode,Json<TableInvite>)>{let token=token(&headers)?;let input=body(input)?;Ok((StatusCode::CREATED,Json(state.blocking(move|db|db.table_invite(&token,&id,&input)).await?)))}
#[utoipa::path(post,path="/api/v1/tables/join",request_body=JoinTable,security(("bearer"=[])),responses((status=200,body=Table),(status=410,body=ErrorBody)),tag="tables")]
async fn join(State(state):State<AppState>,headers:HeaderMap,input:std::result::Result<Json<JoinTable>,JsonRejection>)->Result<Json<Table>>{let token=token(&headers)?;let input=body(input)?;state.blocking(move|db|db.join_table(&token,&input)).await.map(Json)}
#[derive(OpenApi)]
#[openapi(paths(create,list,detail,invite,join),components(schemas(CreateTable,Table,InviteRequest,TableInvite,JoinTable,ErrorBody)))]
pub(crate) struct TableContract;
