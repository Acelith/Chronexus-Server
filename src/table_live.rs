//! Server-authoritative table channel. Every event is selected for the recipient's seat.
use crate::{
    AppState,
    auth::{digest, now, random_secret},
    campaigns::{identifier, text},
    db::Database,
    error::{ApiError, ErrorBody, Result},
    routes::token,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

pub(crate) struct Hub {
    sender: broadcast::Sender<String>,
    /// Transient map pings: (table, frame). Never stored.
    pings: broadcast::Sender<(String, serde_json::Value)>,
    seats: Mutex<HashMap<(String, String), usize>>,
}
impl Hub {
    pub fn new() -> Self {
        Self {
            sender: broadcast::channel(128).0,
            pings: broadcast::channel(64).0,
            seats: Mutex::new(HashMap::new()),
        }
    }
    fn enter(self: &Arc<Self>, table: &str, account: &str) -> Result<Guard> {
        let mut seats = self.seats.lock().map_err(|_| ApiError::internal())?;
        if seats.values().sum::<usize>() >= 64 {
            return Err(ApiError::limited());
        }
        *seats
            .entry((table.to_owned(), account.to_owned()))
            .or_default() += 1;
        drop(seats);
        let _ = self.sender.send(table.to_owned());
        Ok(Guard {
            hub: self.clone(),
            table: table.to_owned(),
            account: account.to_owned(),
        })
    }
    fn count(&self, table: &str) -> usize {
        self.seats
            .lock()
            .map(|seats| seats.iter().filter(|((id, _), _)| id == table).count())
            .unwrap_or(0)
    }
    pub(crate) fn notify(&self, table: &str) {
        let _ = self.sender.send(table.to_owned());
    }
}
struct Guard {
    hub: Arc<Hub>,
    table: String,
    account: String,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut seats) = self.hub.seats.lock() {
            let key = (self.table.clone(), self.account.clone());
            if let Some(count) = seats.get_mut(&key) {
                *count -= 1;
                if *count == 0 {
                    seats.remove(&key);
                }
            }
        }
        let _ = self.hub.sender.send(self.table.clone());
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LiveTicket {
    pub ticket: String,
    pub expires_at: String,
}
#[derive(Deserialize)]
struct TicketQuery {
    ticket: String,
    since: Option<i64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Operation {
    operation_id: String,
    base_revision: i64,
    text: String,
    audience: String,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvent {
    pub revision: i64,
    pub operation_id: String,
    pub author_id: String,
    pub author_name: String,
    pub audience: String,
    pub text: String,
    /// Present only for `/r` and `/gr`; rolled by the server.
    pub roll: Option<crate::table_chat::ChatRoll>,
    pub created_at: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ping {
    #[serde(rename = "type")]
    kind: String,
    scene_id: String,
    x: f64,
    y: f64,
}
struct Identity {
    account: String,
    session: String,
    role: String,
}

impl Database {
    pub fn live_ticket(&self, token: &str, table: &str) -> Result<LiveTicket> {
        identifier(table)?;
        let user = self.identity(token)?;
        let ticket = random_secret()?;
        self.with(|conn| {
            let seat:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM table_seats WHERE table_id=?1 AND account_id=?2)",params![table,user.account_id],|r|r.get(0))?;
            if !seat { return Err(ApiError(StatusCode::FORBIDDEN,"Table access denied.")); }
            conn.execute("DELETE FROM table_live_tickets WHERE expires_at<=?1",[now()])?;
            conn.execute("DELETE FROM table_live_tickets WHERE table_id=?1 AND account_id=?2",params![table,user.account_id])?;
            let expires=now()+30;
            conn.execute("INSERT INTO table_live_tickets(ticket_hash,table_id,account_id,session_id,expires_at) VALUES(?1,?2,?3,?4,?5)",params![digest(&ticket),table,user.account_id,user.session_id,expires])?;
            Ok(LiveTicket{ticket,expires_at:crate::campaigns::timestamp(expires)})
        })
    }
    fn consume_ticket(&self, table: &str, ticket: &str) -> Result<Identity> {
        identifier(table)?;
        if ticket.len() != 64
            || !ticket
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(ApiError::unauthorized());
        }
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row:Option<(String,String,String)>=tx.query_row("SELECT t.account_id,t.session_id,s.role FROM table_live_tickets t JOIN sessions x ON x.id=t.session_id AND x.account_id=t.account_id JOIN accounts a ON a.id=t.account_id JOIN table_seats s ON s.table_id=t.table_id AND s.account_id=t.account_id WHERE t.ticket_hash=?1 AND t.table_id=?2 AND t.expires_at>?3 AND x.revoked_at IS NULL AND x.expires_at>?3 AND x.absolute_expires_at>?3 AND a.disabled=0",params![digest(ticket),table,now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let (account,session,role)=row.ok_or_else(ApiError::unauthorized)?;
            tx.execute("DELETE FROM table_live_tickets WHERE ticket_hash=?1",[digest(ticket)])?;
            tx.commit()?;Ok(Identity{account,session,role})
        })
    }
    fn live_authorized(&self, table: &str, user: &Identity) -> Result<bool> {
        self.with(|conn|Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM table_seats seat JOIN accounts a ON a.id=seat.account_id JOIN sessions s ON s.account_id=a.id WHERE seat.table_id=?1 AND seat.account_id=?2 AND seat.role=?3 AND s.id=?4 AND s.revoked_at IS NULL AND s.expires_at>?5 AND s.absolute_expires_at>?5 AND a.disabled=0)",params![table,user.account,user.role,user.session,now()],|r|r.get(0))?))
    }
    #[cfg(test)]
    fn live_revision(&self, table: &str) -> Result<i64> {
        self.with(|conn| {
            Ok(conn.query_row(
                "SELECT revision FROM game_tables WHERE id=?1",
                [table],
                |r| r.get(0),
            )?)
        })
    }
    fn live_events(
        &self,
        table: &str,
        role: &str,
        account: &str,
        since: i64,
    ) -> Result<Vec<LiveEvent>> {
        // GM-audience lines (whispers, secret rolls) reach the GM and their own author only.
        self.with(|conn| {
            let mut q=conn.prepare("SELECT e.revision,e.operation_id,e.author_id,a.display_name,e.audience,e.text,e.roll,e.created_at FROM table_live_events e JOIN accounts a ON a.id=e.author_id WHERE e.table_id=?1 AND e.revision>?2 AND (e.audience='public' OR ?3='gm' OR e.author_id=?4) ORDER BY e.revision LIMIT 1001")?;
            Ok(q.query_map(params![table,since,role,account],|r|{
                let roll:Option<String>=r.get(6)?;
                Ok(LiveEvent{revision:r.get(0)?,operation_id:r.get(1)?,author_id:r.get(2)?,author_name:r.get(3)?,audience:r.get(4)?,text:r.get(5)?,roll:roll.and_then(|v|serde_json::from_str(&v).ok()),created_at:crate::campaigns::timestamp(r.get(7)?)})
            })?.collect::<std::result::Result<Vec<_>,_>>()?)
        })
    }
    /// Validates a ping on the active scene and returns the author's display name.
    fn ping_author(&self, table: &str, user: &Identity, ping: &Ping) -> Result<String> {
        identifier(&ping.scene_id)?;
        if ping.kind != "ping"
            || user.role == "spectator"
            || !ping.x.is_finite()
            || !ping.y.is_finite()
        {
            return Err(ApiError::invalid());
        }
        if !self.live_authorized(table, user)? {
            return Err(ApiError::unauthorized());
        }
        self.with(|conn| {
            let active: Option<String> = conn
                .query_row(
                    "SELECT active_scene FROM table_boards WHERE table_id=?1",
                    [table],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            if active.as_deref() != Some(ping.scene_id.as_str()) {
                return Err(ApiError(
                    StatusCode::CONFLICT,
                    "Pings are limited to the active scene.",
                ));
            }
            let data: String = conn.query_row(
                "SELECT data FROM table_scenes WHERE id=?1 AND table_id=?2",
                params![ping.scene_id, table],
                |r| r.get(0),
            )?;
            let scene: crate::vtt::Scene =
                serde_json::from_str(&data).map_err(|_| ApiError::internal())?;
            if ping.x < 0.0
                || ping.y < 0.0
                || ping.x > f64::from(scene.width)
                || ping.y > f64::from(scene.height)
            {
                return Err(ApiError::invalid());
            }
            Ok(conn.query_row(
                "SELECT display_name FROM accounts WHERE id=?1",
                [&user.account],
                |r| r.get(0),
            )?)
        })
    }
    fn write_live(&self, table: &str, user: &Identity, op: &Operation) -> Result<i64> {
        identifier(table)?;
        identifier(&op.operation_id)?;
        if Uuid::parse_str(&op.operation_id).is_ok_and(|id| id.get_version_num() != 7)
            || op.base_revision < 1
            || op.audience != "public"
            || user.role == "spectator"
        {
            return Err(ApiError::invalid());
        }
        text(&op.text, 1, 500)?;
        // The server, not the client, decides who may read a line and rolls its dice.
        let (audience, roll) = match crate::table_chat::command(&op.text) {
            crate::table_chat::Command::Say => ("public", None),
            crate::table_chat::Command::Whisper => ("gm", None),
            crate::table_chat::Command::Roll { expression, secret } => (
                if secret { "gm" } else { "public" },
                Some(crate::table_chat::roll(expression, crate::combat::dice)?),
            ),
        };
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let authorized:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM table_seats seat JOIN accounts a ON a.id=seat.account_id JOIN sessions s ON s.account_id=a.id WHERE seat.table_id=?1 AND seat.account_id=?2 AND seat.role=?3 AND s.id=?4 AND s.revoked_at IS NULL AND s.expires_at>?5 AND s.absolute_expires_at>?5 AND a.disabled=0)",params![table,user.account,user.role,user.session,now()],|r|r.get(0))?;
            if !authorized {return Err(ApiError::unauthorized());}
            let expected=audience;
            let existing:Option<(i64,String,String,String,i64)>=tx.query_row("SELECT revision,author_id,audience,text,base_revision FROM table_live_events WHERE table_id=?1 AND operation_id=?2",params![table,op.operation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            if let Some((revision,author,audience,text,base))=existing {
                if author==user.account && audience==expected && text==op.text && base==op.base_revision {return Ok(revision);}
                return Err(ApiError(StatusCode::CONFLICT,"Operation ID already used for different content."));
            }
            let current:i64=tx.query_row("SELECT revision FROM game_tables WHERE id=?1",[table],|r|r.get(0))?;
            if current!=op.base_revision { return Err(ApiError(StatusCode::CONFLICT,"Table revision changed; reload before sending.")); }
            let revision=current+1;
            tx.execute("UPDATE game_tables SET revision=?1 WHERE id=?2",params![revision,table])?;
            let roll=roll.as_ref().map(serde_json::to_string).transpose().map_err(|_|ApiError::internal())?;
            tx.execute("INSERT INTO table_live_events(table_id,revision,operation_id,base_revision,author_id,audience,text,created_at,roll) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![table,revision,op.operation_id,op.base_revision,user.account,audience,op.text,now(),roll])?;
            tx.execute("DELETE FROM table_live_events WHERE table_id=?1 AND revision<?2",params![table,revision-1000])?;
            tx.commit()?;Ok(revision)
        })
    }
}
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/tables/{tableId}/live-ticket", post(ticket))
        .route("/api/v1/tables/{tableId}/live", get(upgrade))
}
#[utoipa::path(post,path="/api/v1/tables/{tableId}/live-ticket",params(("tableId"=String,Path)),security(("bearer"=[])),responses((status=200,body=LiveTicket),(status=403,body=ErrorBody)),tag="tables")]
async fn ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<LiveTicket>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.live_ticket(&token, &id))
        .await
        .map(Json)
}
async fn upgrade(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TicketQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    let ticket = query.ticket;
    let id_for_check = id.clone();
    let user = state
        .blocking(move |db| db.consume_ticket(&id_for_check, &ticket))
        .await?;
    let guard = state.table_live.enter(&id, &user.account)?;
    Ok(ws
        .max_message_size(256 * 1024)
        .max_frame_size(256 * 1024)
        .on_upgrade(move |socket| handle(state, id, user, guard, query.since, socket)))
}
async fn send(socket: &mut WebSocket, value: serde_json::Value) -> bool {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .is_ok()
}
async fn push(
    state: &AppState,
    table: &str,
    user: &Identity,
    socket: &mut WebSocket,
    since: i64,
) -> Option<i64> {
    let id = table.to_owned();
    let role = user.role.clone();
    let account = user.account.clone();
    let session = user.session.clone();
    let result = state
        .blocking(move |db| {
            if !db.live_authorized(
                &id,
                &Identity {
                    account: account.clone(),
                    session,
                    role: role.clone(),
                },
            )? {
                return Err(ApiError::unauthorized());
            }
            let board = db.board_for_account(&id, &account)?;
            let revision = board.revision;
            let oldest = db.with(|conn| {
                Ok(conn.query_row(
                    "SELECT min(revision) FROM table_live_events WHERE table_id=?1",
                    [&id],
                    |r| r.get::<_, Option<i64>>(0),
                )?)
            })?;
            let reset = oldest.is_some_and(|old| since < old - 1);
            let events = db.live_events(&id, &role, &account, if reset { 0 } else { since })?;
            Ok((revision, events, reset, board))
        })
        .await;
    match result { Ok((revision,events,reset,board))=>send(socket,serde_json::json!({"type":"state","revision":revision,"events":events,"reset":reset,"board":board,"online":state.table_live.count(table)})).await.then_some(revision),Err(_)=>None }
}
async fn handle(
    state: AppState,
    table: String,
    user: Identity,
    _guard: Guard,
    since: Option<i64>,
    mut socket: WebSocket,
) {
    let mut cursor = since.unwrap_or(0).max(0);
    let mut subscription = state.table_live.sender.subscribe();
    let mut pings = state.table_live.pings.subscribe();
    let mut last_ping = tokio::time::Instant::now() - Duration::from_secs(2);
    let Some(initial) = push(&state, &table, &user, &mut socket, cursor).await else {
        return;
    };
    cursor = initial;
    let mut timer = tokio::time::interval(Duration::from_secs(20));
    let deadline = tokio::time::sleep(Duration::from_secs(10 * 60));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _=&mut deadline=>break,
            _=timer.tick()=>{
                let id=table.clone();let account=user.account.clone();let session=user.session.clone();let role=user.role.clone();
                if !state.blocking(move|db|db.live_authorized(&id,&Identity{account,session,role})).await.unwrap_or(false) { break; }
                if !send(&mut socket,serde_json::json!({"type":"presence","online":state.table_live.count(&table)})).await { break; }
            }
            signal=subscription.recv()=>{
                if matches!(&signal,Ok(id) if id==&table) || matches!(&signal,Err(broadcast::error::RecvError::Lagged(_))) {
                    let id=table.clone();let account=user.account.clone();let session=user.session.clone();let role=user.role.clone();
                    if !state.blocking(move|db|db.live_authorized(&id,&Identity{account,session,role})).await.unwrap_or(false) {break;}
                }
                match signal { Ok(id) if id==table=>{let Some(revision)=push(&state,&table,&user,&mut socket,cursor).await else {break};cursor=revision;},Err(broadcast::error::RecvError::Closed)=>break,Err(broadcast::error::RecvError::Lagged(_))=>{cursor=0;let Some(revision)=push(&state,&table,&user,&mut socket,cursor).await else {break};cursor=revision;},_=>{} }
            }
            ping=pings.recv()=>{
                if let Ok((id,frame))=ping && id==table && !send(&mut socket,frame).await {break;}
            }
            message=socket.recv()=>{
                let Some(Ok(Message::Text(raw)))=message else {break};
                if let Ok(ping)=serde_json::from_str::<Ping>(raw.as_str()) {
                    // At most one ping per connection every 750 ms; extra pings are dropped.
                    if last_ping.elapsed()<Duration::from_millis(750) {continue;}
                    last_ping=tokio::time::Instant::now();
                    let id=table.clone();
                    let identity=Identity{account:user.account.clone(),session:user.session.clone(),role:user.role.clone()};
                    let (scene,x,y)=(ping.scene_id.clone(),ping.x,ping.y);
                    match state.blocking(move|db|db.ping_author(&id,&identity,&ping)).await {
                        Ok(name)=>{let _=state.table_live.pings.send((table.clone(),serde_json::json!({"type":"ping","sceneId":scene,"x":x,"y":y,"authorId":user.account,"authorName":name})));}
                        Err(error)=>{if !send(&mut socket,serde_json::json!({"type":"pingError","message":error.1})).await{break;}}
                    }
                    continue;
                }
                if let Ok(op)=serde_json::from_str::<crate::vtt::BoardOperation>(raw.as_str()) {
                    let id=table.clone();let account=user.account.clone();let session=user.session.clone();let role=user.role.clone();
                    let result=state.blocking(move|db|{if !db.live_authorized(&id,&Identity{account:account.clone(),session,role})? {return Err(ApiError::unauthorized());}db.board_write(&id,&account,&op)}).await;
                    match result {Ok(ack)=>{state.table_live.notify(&table);if !send(&mut socket,serde_json::json!({"type":"boardAck","revision":ack.revision})).await{break;}},Err(error)=>{if !send(&mut socket,serde_json::json!({"type":"boardError","message":error.1})).await{break;}}}
                    continue;
                }
                let Ok(op)=serde_json::from_str::<Operation>(raw.as_str()) else { if !send(&mut socket,serde_json::json!({"type":"error","message":"Invalid operation."})).await {break;} continue;};
                let id=table.clone();let account=user.account.clone();let session=user.session.clone();let role=user.role.clone();
                let result=state.blocking(move|db|{let identity=Identity{account,session,role};if !db.live_authorized(&id,&identity)? {return Err(ApiError::unauthorized());}db.write_live(&id,&identity,&op)}).await;
                match result { Ok(revision)=>{let _=state.table_live.sender.send(table.clone());if !send(&mut socket,serde_json::json!({"type":"ack","revision":revision})).await {break;}},Err(error)=>{if !send(&mut socket,serde_json::json!({"type":"error","message":error.1})).await {break;}} }
            }
        }
    }
}
#[derive(OpenApi)]
#[openapi(
    paths(ticket),
    components(schemas(
        LiveTicket,
        LiveEvent,
        crate::table_chat::ChatRoll,
        crate::table_chat::RollTerm,
        ErrorBody
    ))
)]
pub(crate) struct LiveContract;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::Redeem,
        tables::{CreateTable, InviteRequest, JoinTable},
    };

    #[test]
    fn ticket_and_operation_are_one_use_and_seat_scoped() {
        let directory = tempfile::tempdir_in("/private/tmp")
            .or_else(|_| tempfile::tempdir())
            .unwrap();
        let db = Database::open(&directory.path().join("live.sqlite3")).unwrap();
        let gm_code = db.issue_invite_with_gm(None, true).unwrap();
        let gm = db
            .redeem(
                &Redeem {
                    invite_code: gm_code,
                    username: "live-gm".into(),
                    display_name: "GM".into(),
                    password: "synthetic-password-123".into(),
                    device: "test".into(),
                },
                "127.0.0.1",
            )
            .unwrap();
        let player_code = db.issue_invite(None).unwrap();
        let player = db
            .redeem(
                &Redeem {
                    invite_code: player_code,
                    username: "live-player".into(),
                    display_name: "Player".into(),
                    password: "synthetic-password-123".into(),
                    device: "test".into(),
                },
                "127.0.0.2",
            )
            .unwrap();
        let table = db
            .create_table(
                &gm.token,
                &CreateTable {
                    name: "Synthetic".into(),
                },
            )
            .unwrap();
        let invite = db
            .table_invite(
                &gm.token,
                &table.id,
                &InviteRequest {
                    role: "player".into(),
                },
            )
            .unwrap();
        db.join_table(
            &player.token,
            &JoinTable {
                invite_code: invite.code,
            },
        )
        .unwrap();
        let ticket = db.live_ticket(&player.token, &table.id).unwrap();
        let identity = db.consume_ticket(&table.id, &ticket.ticket).unwrap();
        assert_eq!(identity.role, "player");
        assert!(db.consume_ticket(&table.id, &ticket.ticket).is_err());
        let op = Operation {
            operation_id: Uuid::now_v7().to_string(),
            base_revision: 2,
            text: "Public table message".into(),
            audience: "public".into(),
        };
        assert_eq!(db.write_live(&table.id, &identity, &op).unwrap(), 3);
        assert_eq!(db.write_live(&table.id, &identity, &op).unwrap(), 3);
        let changed = Operation {
            text: "Different text".into(),
            ..op
        };
        assert_eq!(
            db.write_live(&table.id, &identity, &changed).unwrap_err().0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            db.live_events(&table.id, "player", &identity.account, 2)
                .unwrap()
                .len(),
            1
        );
        let secret = Operation {
            operation_id: Uuid::now_v7().to_string(),
            base_revision: 3,
            text: "Secret".into(),
            audience: "gm".into(),
        };
        assert_eq!(
            db.write_live(&table.id, &identity, &secret).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        let spectator_code = db.issue_invite(None).unwrap();
        let spectator = db
            .redeem(
                &Redeem {
                    invite_code: spectator_code,
                    username: "live-spectator".into(),
                    display_name: "Spectator".into(),
                    password: "synthetic-password-123".into(),
                    device: "test".into(),
                },
                "127.0.0.3",
            )
            .unwrap();
        let seat_code = db
            .table_invite(
                &gm.token,
                &table.id,
                &InviteRequest {
                    role: "spectator".into(),
                },
            )
            .unwrap();
        db.join_table(
            &spectator.token,
            &JoinTable {
                invite_code: seat_code.code,
            },
        )
        .unwrap();
        let spectator_ticket = db.live_ticket(&spectator.token, &table.id).unwrap();
        let watcher = db
            .consume_ticket(&table.id, &spectator_ticket.ticket)
            .unwrap();
        let spectator_op = Operation {
            operation_id: Uuid::now_v7().to_string(),
            base_revision: db.live_revision(&table.id).unwrap(),
            text: "Cannot write".into(),
            audience: "public".into(),
        };
        assert_eq!(
            db.write_live(&table.id, &watcher, &spectator_op)
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
        // Chat commands: the server rolls and narrows the audience; nobody else sees whispers.
        let say = |text: &str, who: &Identity| {
            let op = Operation {
                operation_id: Uuid::now_v7().to_string(),
                base_revision: db.live_revision(&table.id).unwrap(),
                text: text.into(),
                audience: "public".into(),
            };
            let revision = db.write_live(&table.id, who, &op).unwrap();
            assert_eq!(db.write_live(&table.id, who, &op).unwrap(), revision);
            revision
        };
        let gm_ticket = db.live_ticket(&gm.token, &table.id).unwrap();
        let gm_identity = db.consume_ticket(&table.id, &gm_ticket.ticket).unwrap();
        let before = db.live_revision(&table.id).unwrap();
        say("/r 2d20kh1+5 Attack", &identity);
        say("/gr 1d6", &identity);
        say("/w gm I pocket the gem", &identity);
        let player_view = db
            .live_events(&table.id, "player", &identity.account, before)
            .unwrap();
        let roll = player_view[0].roll.as_ref().unwrap();
        assert_eq!(roll.label, "Attack");
        assert!((6..=25).contains(&roll.total));
        assert_eq!(roll.terms[0].kept.iter().filter(|k| **k).count(), 1);
        assert_eq!(player_view[1].audience, "gm");
        assert_eq!(player_view[2].audience, "gm");
        assert!(player_view[2].roll.is_none());
        assert_eq!(
            db.live_events(&table.id, "gm", &gm_identity.account, before)
                .unwrap()
                .len(),
            3
        );
        let spectator_view = db
            .live_events(&table.id, "spectator", &watcher.account, before)
            .unwrap();
        assert_eq!(spectator_view.len(), 1);
        assert!(spectator_view[0].roll.is_some());
        let invalid = Operation {
            operation_id: Uuid::now_v7().to_string(),
            base_revision: db.live_revision(&table.id).unwrap(),
            text: "/r 500d6".into(),
            audience: "public".into(),
        };
        assert_eq!(
            db.write_live(&table.id, &identity, &invalid).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        // Pings need an active scene inside its bounds, and never come from spectators.
        let scene = Uuid::new_v4().to_string();
        let ping = |x: f64| Ping {
            kind: "ping".into(),
            scene_id: scene.clone(),
            x,
            y: 10.0,
        };
        assert_eq!(
            db.ping_author(&table.id, &identity, &ping(10.0))
                .unwrap_err()
                .0,
            StatusCode::CONFLICT
        );
        db.with(|conn| {
            conn.execute(
                "INSERT INTO table_scenes(id,table_id,data) VALUES(?1,?2,?3)",
                params![
                    scene,
                    table.id,
                    serde_json::json!({"id":scene,"name":"S","width":100,"height":100,"mapId":null,"grid":{"kind":"square","pixels":50,"offset":{"x":0,"y":0},"color":"#ffffff","opacity":0.5,"distance":{"value":5,"unit":"ft"},"diagonal":"equal","conversion":"table"},"freeMove":true,"difficult":[]}).to_string()
                ],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.ping_author(&table.id, &identity, &ping(10.0))
                .unwrap_err()
                .0,
            StatusCode::CONFLICT
        );
        db.with(|conn| {
            conn.execute(
                "INSERT INTO table_boards(table_id,active_scene) VALUES(?1,?2)",
                params![table.id, scene],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.ping_author(&table.id, &identity, &ping(10.0)).unwrap(),
            "Player"
        );
        assert!(db.ping_author(&table.id, &identity, &ping(500.0)).is_err());
        assert!(
            db.ping_author(&table.id, &identity, &ping(f64::NAN))
                .is_err()
        );
        assert!(db.ping_author(&table.id, &watcher, &ping(10.0)).is_err());
        db.remove_table_seat(&gm.token, &table.id, &player.account_id)
            .unwrap();
        assert!(!db.live_authorized(&table.id, &identity).unwrap());
    }
}
