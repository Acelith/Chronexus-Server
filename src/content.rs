use crate::{
    auth::now,
    campaigns::{forbidden, identifier, member, missing, owner, timestamp},
    db::Database,
    error::{ApiError, ErrorBody, Result},
    payload::Payload,
};
use axum::http::StatusCode;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

/// Request IDs are UUIDv7 so their age is verifiable: older IDs are refused, which is
/// what makes expiring their receipts safe (a late retry cannot be silently re-applied).
const RECEIPT_WINDOW: i64 = 30 * 86_400;
const RECEIPT_RETENTION: i64 = 31 * 86_400;
const RECEIPT_CAP: i64 = 100_000;
/// Log rows kept per campaign before the oldest are dropped and old cursors expire.
pub const LOG_KEEP: i64 = 100_000;
const SYNC_ROWS: i64 = 500;
const SYNC_BYTES: usize = 6 * 1024 * 1024;

const LEGACY: &[&str] = &["page", "session", "relationship", "party", "clue"];
#[derive(Clone, Copy, Default, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Private,
    Members,
}
impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Members => "members",
        }
    }
}
/// Query for keyed deletions and sharing withdrawals.
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyedRevision {
    pub base_revision: i64,
    /// Stable UUIDv7 operation ID. Reuse only for an identical retry.
    #[serde(default)]
    pub request_id: Option<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteItem {
    pub base_revision: i64,
    pub payload: Payload,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteContent {
    /// Stable UUIDv7 operation ID. Reuse only for an identical save.
    #[serde(default)]
    pub request_id: Option<String>,
    pub base_revision: i64,
    pub payload: Payload,
    #[serde(default)]
    pub visibility: Visibility,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub revision: i64,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub revision: i64,
    pub payload: Option<Payload>,
}
/// Save conflicts either contain the current version or reject request ID reuse.
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub enum SaveConflict {
    Current(Current),
    Request(ErrorBody),
}
pub enum Written {
    Accepted(Revision),
    Conflict(Current),
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub kind: String,
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub revision: i64,
    pub deleted: bool,
    pub payload: Option<Payload>,
    pub updated_at: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentItem {
    #[serde(flatten)]
    pub item: Item,
    pub visibility: Visibility,
}
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Changes {
    pub items: Vec<Item>,
    pub cursor: String,
    pub more: bool,
}
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ContentList {
    pub items: Vec<ContentItem>,
    pub cursor: String,
    pub more: bool,
    /// Sync cursor at the time of this page; resume `/sync` from the first page's value.
    pub head: String,
}
/// Incremental authorized state. Tombstones (`deleted`) carry no payload; for other
/// members they also cover items that stopped being shared.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct SyncPage {
    pub items: Vec<ContentItem>,
    pub cursor: String,
    pub more: bool,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    #[serde(default)]
    pub cursor: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Since {
    #[serde(default)]
    pub since: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct After {
    #[serde(default)]
    pub after: String,
}

struct Stored {
    author: String,
    revision: i64,
    visibility: Visibility,
    payload: Option<Payload>,
    deleted: bool,
}
fn stored(conn: &Connection, campaign: &str, kind: &str, id: &str) -> Result<Option<Stored>> {
    let row:Option<(String,i64,String,Option<String>,bool)>=conn.query_row("SELECT author_id,revision,visibility,payload,deleted FROM content WHERE campaign_id=?1 AND kind=?2 AND id=?3",params![campaign,kind,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    row.map(|(author, revision, visibility, payload, deleted)| {
        Ok(Stored {
            author,
            revision,
            visibility: if visibility == "members" {
                Visibility::Members
            } else {
                Visibility::Private
            },
            payload: payload
                .map(|p| serde_json::from_str(&p).map_err(|_| ApiError::internal()))
                .transpose()?,
            deleted,
        })
    })
    .transpose()
}
pub(crate) fn writable(conn: &Connection, campaign: &str, account: &str) -> Result<()> {
    if member(conn, campaign, account)? == "reader" {
        Err(forbidden())
    } else {
        Ok(())
    }
}
fn kind(value: &str) -> Result<()> {
    if LEGACY.contains(&value) || ["sheetSection", "image"].contains(&value) {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}
pub(crate) fn publish(
    conn: &Connection,
    campaign: &str,
    kind: &str,
    id: &str,
    revision: i64,
    deleted: bool,
) -> Result<()> {
    if !LEGACY.contains(&kind) {
        return Ok(());
    }
    let seq: i64 = conn.query_row(
        "UPDATE campaigns SET next_seq=next_seq+1 WHERE id=?1 RETURNING next_seq",
        [campaign],
        |r| r.get(0),
    )?;
    conn.execute("INSERT INTO published_items(campaign_id,kind,id,revision,deleted,seq,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(campaign_id,kind,id) DO UPDATE SET revision=excluded.revision,deleted=excluded.deleted,seq=excluded.seq,updated_at=excluded.updated_at",params![campaign,kind,id,revision,deleted,seq,now()])?;
    Ok(())
}
fn dependencies(
    conn: &Connection,
    campaign: &str,
    kind: &str,
    id: &str,
    author: &str,
    input: &WriteContent,
) -> Result<()> {
    for target in input.payload.pages() {
        let page = stored(conn, campaign, "page", target)?.ok_or_else(missing)?;
        if page.deleted || (page.visibility == Visibility::Private && page.author != author) {
            return Err(missing());
        }
        if input.visibility == Visibility::Members && page.visibility != Visibility::Members {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "Share the referenced pages explicitly first.",
            ));
        }
    }
    if let Payload::Clue(clue) = &input.payload {
        conn.execute(
            "DELETE FROM clue_session_dependencies WHERE campaign_id=?1 AND clue_id=?2",
            params![campaign, id],
        )?;
        for source in &clue.sources {
            if source.kind != "session" {
                continue;
            }
            let session = stored(conn, campaign, "session", &source.id)?.ok_or_else(missing)?;
            if session.deleted
                || (session.visibility == Visibility::Private && session.author != author)
            {
                return Err(missing());
            }
            if input.visibility == Visibility::Members && session.visibility != Visibility::Members
            {
                return Err(ApiError(
                    StatusCode::BAD_REQUEST,
                    "Share the referenced sessions explicitly first.",
                ));
            }
            conn.execute("INSERT OR IGNORE INTO clue_session_dependencies(campaign_id,clue_id,session_id) VALUES(?1,?2,?3)", params![campaign,id,source.id])?;
        }
        if let Some(session_id) = &clue.resolution_session_id {
            let session = stored(conn, campaign, "session", session_id)?.ok_or_else(missing)?;
            if session.deleted
                || (session.visibility == Visibility::Private && session.author != author)
            {
                return Err(missing());
            }
            if input.visibility == Visibility::Members && session.visibility != Visibility::Members
            {
                return Err(ApiError(
                    StatusCode::BAD_REQUEST,
                    "Share the referenced sessions explicitly first.",
                ));
            }
            conn.execute("INSERT OR IGNORE INTO clue_session_dependencies(campaign_id,clue_id,session_id) VALUES(?1,?2,?3)", params![campaign,id,session_id])?;
        }
    }
    if let Payload::Party(p) = &input.payload
        && p.page_id != id
    {
        return Err(ApiError::invalid());
    }
    if let Payload::Image(image) = &input.payload {
        let own:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM attachments WHERE id=?1 AND campaign_id=?2 AND author_id=?3)",params![image.attachment_id,campaign,author],|r|r.get(0))?;
        if !own {
            return Err(missing());
        }
    }
    conn.execute(
        "DELETE FROM content_dependencies WHERE campaign_id=?1 AND kind=?2 AND item_id=?3",
        params![campaign, kind, id],
    )?;
    for target in input.payload.pages() {
        conn.execute(
            "INSERT OR IGNORE INTO content_dependencies(campaign_id,kind,item_id,page_id) VALUES(?1,?2,?3,?4)",
            params![campaign, kind, id, target],
        )?;
    }
    conn.execute(
        "DELETE FROM content_attachments WHERE campaign_id=?1 AND kind=?2 AND item_id=?3",
        params![campaign, kind, id],
    )?;
    if let Payload::Image(image) = &input.payload {
        conn.execute(
            "INSERT INTO content_attachments(campaign_id,kind,item_id,attachment_id) VALUES(?1,?2,?3,?4)",
            params![campaign, kind, id, image.attachment_id],
        )?;
    }
    Ok(())
}
fn withdraw(conn: &Connection, campaign: &str, kind: &str, id: &str) -> Result<i64> {
    let revision:i64=conn.query_row("UPDATE content SET visibility='private',revision=revision+1,updated_at=?1 WHERE campaign_id=?2 AND kind=?3 AND id=?4 RETURNING revision",params![now(),campaign,kind,id],|r|r.get(0))?;
    publish(conn, campaign, kind, id, revision, true)?;
    let author: String = conn.query_row(
        "SELECT author_id FROM content WHERE campaign_id=?1 AND kind=?2 AND id=?3",
        params![campaign, kind, id],
        |r| r.get(0),
    )?;
    log_change(conn, campaign, kind, id, &author, "withdrawn")?;
    Ok(revision)
}
/// Withdraws the published items that depend on `page` and returns how many there were.
pub(crate) fn withdraw_dependents(conn: &Connection, campaign: &str, page: &str) -> Result<usize> {
    let mut q=conn.prepare("SELECT c.kind,c.id FROM content c JOIN content_dependencies d ON d.campaign_id=c.campaign_id AND d.kind=c.kind AND d.item_id=c.id WHERE c.campaign_id=?1 AND d.page_id=?2 AND c.visibility='members' AND c.deleted=0")?;
    let rows = q
        .query_map(params![campaign, page], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (kind, id) in &rows {
        withdraw(conn, campaign, kind, id)?;
    }
    Ok(rows.len())
}
pub(crate) fn withdraw_session_dependents(
    conn: &Connection,
    campaign: &str,
    session: &str,
) -> Result<usize> {
    let mut q = conn.prepare("SELECT c.id FROM content c JOIN clue_session_dependencies d ON d.campaign_id=c.campaign_id AND d.clue_id=c.id WHERE c.campaign_id=?1 AND c.kind='clue' AND d.session_id=?2 AND c.visibility='members' AND c.deleted=0")?;
    let ids = q
        .query_map(params![campaign, session], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for id in &ids {
        withdraw(conn, campaign, "clue", id)?;
    }
    Ok(ids.len())
}
pub(crate) fn withdraw_author(conn: &Connection, campaign: &str, account: &str) -> Result<()> {
    let mut q=conn.prepare("SELECT kind,id FROM content WHERE campaign_id=?1 AND author_id=?2 AND visibility='members' AND deleted=0")?;
    let rows = q
        .query_map(params![campaign, account], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (kind, id) in rows {
        if stored(conn, campaign, &kind, &id)?.is_some_and(|r| r.visibility == Visibility::Members)
        {
            withdraw(conn, campaign, &kind, &id)?;
        }
        if kind == "page" {
            withdraw_dependents(conn, campaign, &id)?;
        } else if kind == "session" {
            withdraw_session_dependents(conn, campaign, &id)?;
        }
    }
    Ok(())
}
/// Appends a content-free log row; returns whether every member may see it.
pub(crate) fn log_change(
    conn: &Connection,
    campaign: &str,
    kind: &str,
    id: &str,
    author: &str,
    state: &str,
) -> Result<bool> {
    let seq: i64 = conn.query_row(
        "UPDATE campaigns SET next_seq=next_seq+1 WHERE id=?1 RETURNING next_seq",
        [campaign],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO content_log(campaign_id,seq,kind,id,author_id,state,at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![campaign, seq, kind, id, author, state, now()],
    )?;
    Ok(state != "private" && state != "deleted-private")
}
fn request_id(value: &str) -> Result<()> {
    identifier(value)?;
    let uuid = Uuid::parse_str(value).map_err(|_| ApiError::invalid())?;
    if uuid.get_version_num() != 7 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Request IDs must be UUIDv7.",
        ));
    }
    let seconds = (uuid.as_u128() >> 80) as i64 / 1000;
    if seconds > now() + 300 {
        return Err(ApiError::invalid());
    }
    if seconds < now() - RECEIPT_WINDOW {
        return Err(ApiError(
            StatusCode::GONE,
            "Request ID expired. Re-read the item and create a new operation.",
        ));
    }
    Ok(())
}
fn digest_of(value: serde_json::Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
/// One keyed operation. A previously committed ID is acknowledged with its original
/// revision and never re-applied; unknown IDs get expired receipts compacted and the cap checked.
struct Keyed<'a> {
    campaign: &'a str,
    account: &'a str,
    request: &'a str,
    op: &'a str,
    kind: &'a str,
    id: &'a str,
    digest: String,
}
impl Keyed<'_> {
    fn replay(&self, tx: &Connection) -> Result<Option<i64>> {
        let receipt: Option<(String, String, String, String, i64)> = tx
            .query_row(
                "SELECT op,kind,item_id,request_hash,revision FROM save_receipts WHERE campaign_id=?1 AND account_id=?2 AND request_id=?3",
                params![self.campaign, self.account, self.request],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        if let Some((op, kind, id, hash, revision)) = receipt {
            if op != self.op || kind != self.kind || id != self.id || hash != self.digest {
                return Err(ApiError(
                    StatusCode::CONFLICT,
                    "Request ID already used for a different save.",
                ));
            }
            return Ok(Some(revision));
        }
        tx.execute(
            "DELETE FROM save_receipts WHERE campaign_id=?1 AND account_id=?2 AND created_at<?3",
            params![self.campaign, self.account, now() - RECEIPT_RETENTION],
        )?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM save_receipts WHERE campaign_id=?1 AND account_id=?2",
            params![self.campaign, self.account],
            |r| r.get(0),
        )?;
        if count >= RECEIPT_CAP {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Save receipt quota reached. Preserve pending local changes.",
            ));
        }
        Ok(None)
    }
    fn remember(&self, tx: &Connection, revision: i64) -> Result<()> {
        tx.execute(
            "INSERT INTO save_receipts(campaign_id,account_id,request_id,op,kind,item_id,request_hash,revision,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![self.campaign, self.account, self.request, self.op, self.kind, self.id, self.digest, revision, now()],
        )?;
        Ok(())
    }
}
fn bump(revision: i64) -> Result<i64> {
    revision.checked_add(1).ok_or_else(ApiError::internal)
}
impl Database {
    pub fn account_for(&self, token: &str) -> Result<String> {
        self.identity(token).map(|identity| identity.account_id)
    }
    pub fn authorize_content(&self, token: &str, campaign: &str, write: bool) -> Result<()> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            if write {
                writable(conn, campaign, &identity.account_id)
            } else {
                member(conn, campaign, &identity.account_id).map(|_| ())
            }
        })
    }
    pub fn write_content(
        &self,
        token: &str,
        campaign: &str,
        kind_name: &str,
        id: &str,
        input: &WriteContent,
        legacy: bool,
    ) -> Result<Written> {
        if let Some(request) = &input.request_id {
            request_id(request)?;
        }
        identifier(id)?;
        kind(kind_name)?;
        if legacy && !LEGACY.contains(&kind_name) {
            return Err(ApiError::invalid());
        }
        input.payload.validate(kind_name)?;
        if input.base_revision < 0 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        // Hash normalized JSON so omitted default visibility and object key order are equivalent.
        let normalized =
            serde_json::to_value((&input.payload, input.base_revision, input.visibility))
                .map_err(|_| ApiError::internal())?;
        let digest = digest_of(normalized);
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;writable(&tx,campaign,&identity.account_id)?;
            let keyed = input.request_id.as_deref().map(|request| Keyed {
                campaign,
                account: &identity.account_id,
                request,
                op: "save",
                kind: kind_name,
                id,
                digest: digest.clone(),
            });
            if let Some(keyed) = &keyed
                && let Some(revision) = keyed.replay(&tx)?
            {
                // A receipt acknowledges a historical operation, never restores its content.
                return Ok(Written::Accepted(Revision { revision }));
            }
            let old=stored(&tx,campaign,kind_name,id)?;
            if let Some(old)=&old {
                if old.author!=identity.account_id {return Err(if old.visibility==Visibility::Private{missing()}else{forbidden()});}
                if !(old.revision==input.base_revision || (old.deleted && input.base_revision==0)) {return Ok(Written::Conflict(Current{revision:old.revision,payload:old.payload.clone()}));}
            } else if input.base_revision!=0 {return Err(missing());}
            let payload=serde_json::to_string(&input.payload).map_err(|_|ApiError::internal())?;
            let (count,size):(i64,i64)=tx.query_row("SELECT count(*),coalesce(sum(length(CAST(payload AS BLOB))),0) FROM content WHERE campaign_id=?1",[campaign],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let previous:i64=tx.query_row("SELECT coalesce(length(CAST(payload AS BLOB)),0) FROM content WHERE campaign_id=?1 AND kind=?2 AND id=?3",params![campaign,kind_name,id],|r|r.get(0)).optional()?.unwrap_or(0);
            if (old.is_none() && count>=5000)||size-previous+payload.len() as i64>64*1024*1024 {return Err(ApiError(StatusCode::TOO_MANY_REQUESTS,"Campaign content quota reached."));}
            let revision=old.as_ref().map(|o|bump(o.revision)).transpose()?.unwrap_or(1);
            tx.execute("INSERT INTO content(campaign_id,kind,id,author_id,revision,visibility,payload,deleted,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,0,?8) ON CONFLICT(campaign_id,kind,id) DO UPDATE SET revision=excluded.revision,visibility=excluded.visibility,payload=excluded.payload,deleted=0,updated_at=excluded.updated_at",params![campaign,kind_name,id,identity.account_id,revision,input.visibility.as_str(),payload,now()])?;
            dependencies(&tx,campaign,kind_name,id,&identity.account_id,input)?;
            if input.visibility==Visibility::Members {publish(&tx,campaign,kind_name,id,revision,false)?;}
            else if old.as_ref().is_some_and(|o|o.visibility==Visibility::Members && !o.deleted) {publish(&tx,campaign,kind_name,id,revision,true)?;}
            if kind_name=="page" && input.visibility==Visibility::Private {withdraw_dependents(&tx,campaign,id)?;}
            if kind_name=="session" && input.visibility==Visibility::Private {withdraw_session_dependents(&tx,campaign,id)?;}
            let was_public = old.as_ref().is_some_and(|o| o.visibility == Visibility::Members && !o.deleted);
            let state = match (input.visibility, was_public) {
                (Visibility::Members, _) => "members",
                (Visibility::Private, true) => "withdrawn",
                (Visibility::Private, false) => "private",
            };
            let mut public = log_change(&tx, campaign, kind_name, id, &identity.account_id, state)?;
            public |= kind_name == "page" && input.visibility == Visibility::Private && was_public;
            if let Some(keyed) = &keyed {
                keyed.remember(&tx, revision)?;
            }
            tx.commit()?;
            self.notify(campaign, &identity.account_id, public);
            Ok(Written::Accepted(Revision{revision}))
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn delete_content(
        &self,
        token: &str,
        campaign: &str,
        kind_name: &str,
        id: &str,
        base: i64,
        request: Option<&str>,
        legacy: bool,
    ) -> Result<Written> {
        if let Some(request) = request {
            request_id(request)?;
        }
        identifier(id)?;
        kind(kind_name)?;
        if base < 1 || (legacy && !LEGACY.contains(&kind_name)) {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;writable(&tx,campaign,&identity.account_id)?;
            let keyed = request.map(|request| Keyed {
                campaign,
                account: &identity.account_id,
                request,
                op: "delete",
                kind: kind_name,
                id,
                digest: digest_of(serde_json::json!({"op":"delete","base":base,"legacy":legacy})),
            });
            if let Some(keyed) = &keyed
                && let Some(revision) = keyed.replay(&tx)?
            {
                return Ok(Written::Accepted(Revision { revision }));
            }
            let old=stored(&tx,campaign,kind_name,id)?.ok_or_else(missing)?;
            if old.author!=identity.account_id {return Err(if old.visibility==Visibility::Private{missing()}else{forbidden()});}
            if old.deleted || (legacy&&old.visibility==Visibility::Private){return Err(missing());}
            if old.revision!=base{return Ok(Written::Conflict(Current{revision:old.revision,payload:old.payload}));}
            let revision=bump(base)?;
            let was_public = old.visibility==Visibility::Members;
            if legacy {
                let revision=withdraw(&tx,campaign,kind_name,id)?;
                if kind_name=="page"{withdraw_dependents(&tx,campaign,id)?;}
                if kind_name=="session"{withdraw_session_dependents(&tx,campaign,id)?;}
                if let Some(keyed) = &keyed { keyed.remember(&tx, revision)?; }
                tx.commit()?;
                self.notify(campaign, &identity.account_id, true);
                return Ok(Written::Accepted(Revision{revision}));
            }
            tx.execute("UPDATE content SET deleted=1,payload=NULL,revision=?1,updated_at=?2 WHERE campaign_id=?3 AND kind=?4 AND id=?5",params![revision,now(),campaign,kind_name,id])?;
            for table in ["content_dependencies","content_attachments"] {tx.execute(&format!("DELETE FROM {table} WHERE campaign_id=?1 AND kind=?2 AND item_id=?3"),params![campaign,kind_name,id])?;}
            if kind_name=="clue" {tx.execute("DELETE FROM clue_session_dependencies WHERE campaign_id=?1 AND clue_id=?2",params![campaign,id])?;}
            if was_public{publish(&tx,campaign,kind_name,id,revision,true)?;}
            // Other members hear about the deletion only if something they could see changed.
            let withdrawn = if kind_name=="page"{withdraw_dependents(&tx,campaign,id)?}else if kind_name=="session"{withdraw_session_dependents(&tx,campaign,id)?}else{0};
            let public = log_change(&tx, campaign, kind_name, id, &identity.account_id, if was_public { "deleted" } else { "deleted-private" })?
                || withdrawn > 0;
            if let Some(keyed) = &keyed { keyed.remember(&tx, revision)?; }
            tx.commit()?;
            self.notify(campaign, &identity.account_id, public);
            Ok(Written::Accepted(Revision{revision}))
        })
    }
    pub fn unshare_content(
        &self,
        token: &str,
        campaign: &str,
        kind_name: &str,
        id: &str,
        base: i64,
        request: Option<&str>,
    ) -> Result<Written> {
        if let Some(request) = request {
            request_id(request)?;
        }
        identifier(id)?;
        kind(kind_name)?;
        if base < 1 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            member(&tx, campaign, &identity.account_id)?;
            let keyed = request.map(|request| Keyed {
                campaign,
                account: &identity.account_id,
                request,
                op: "unshare",
                kind: kind_name,
                id,
                digest: digest_of(serde_json::json!({"op":"unshare","base":base})),
            });
            if let Some(keyed) = &keyed
                && let Some(revision) = keyed.replay(&tx)?
            {
                return Ok(Written::Accepted(Revision { revision }));
            }
            let old = stored(&tx, campaign, kind_name, id)?.ok_or_else(missing)?;
            if old.author != identity.account_id {
                if old.visibility == Visibility::Private {
                    return Err(missing());
                }
                owner(&tx, campaign, &identity.account_id)?;
            } else {
                writable(&tx, campaign, &identity.account_id)?;
            }
            if old.deleted || old.visibility == Visibility::Private {
                return Err(missing());
            }
            if old.revision != base {
                // A moderator cannot receive an author's private draft in a conflict response.
                return Ok(Written::Conflict(Current {
                    revision: old.revision,
                    payload: old.payload,
                }));
            }
            let revision = withdraw(&tx, campaign, kind_name, id)?;
            if kind_name == "page" {
                withdraw_dependents(&tx, campaign, id)?;
            } else if kind_name == "session" {
                withdraw_session_dependents(&tx, campaign, id)?;
            }
            if let Some(keyed) = &keyed {
                keyed.remember(&tx, revision)?;
            }
            tx.commit()?;
            self.notify(campaign, &identity.account_id, true);
            Ok(Written::Accepted(Revision { revision }))
        })
    }
    pub fn content_item(
        &self,
        token: &str,
        campaign: &str,
        kind_name: &str,
        id: &str,
    ) -> Result<ContentItem> {
        identifier(id)?;
        kind(kind_name)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            member(conn,campaign,&identity.account_id)?;let stored=stored(conn,campaign,kind_name,id)?.ok_or_else(missing)?;
            if stored.deleted||(stored.visibility==Visibility::Private&&stored.author!=identity.account_id){return Err(missing());}
            let (name,time):(String,i64)=conn.query_row("SELECT a.display_name,c.updated_at FROM content c JOIN accounts a ON a.id=c.author_id WHERE c.campaign_id=?1 AND c.kind=?2 AND c.id=?3",params![campaign,kind_name,id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            Ok(ContentItem{visibility:stored.visibility,item:Item{kind:kind_name.into(),id:id.into(),author_id:stored.author,author_name:name,revision:stored.revision,deleted:false,payload:stored.payload,updated_at:timestamp(time)}})
        })
    }
    pub fn content_list(&self, token: &str, campaign: &str, after: &str) -> Result<ContentList> {
        if !after.is_empty() {
            let (k, id) = after.split_once(':').ok_or_else(ApiError::invalid)?;
            kind(k)?;
            identifier(id)?;
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            member(conn,campaign,&identity.account_id)?;
            let mut q=conn.prepare("SELECT c.kind,c.id,c.author_id,a.display_name,c.revision,c.payload,c.updated_at,c.visibility FROM content c JOIN accounts a ON a.id=c.author_id WHERE c.campaign_id=?1 AND c.deleted=0 AND (c.author_id=?2 OR c.visibility='members') AND c.kind||':'||c.id>?3 ORDER BY c.kind,c.id LIMIT 51")?;
            let mut rows=q.query(params![campaign,identity.account_id,after])?;let mut items=Vec::new();let mut bytes=0;let mut cursor=after.to_string();let mut more=false;
            while let Some(row)=rows.next()? {
                let item=content_row(row)?;let length=serde_json::to_vec(&item).map_err(|_|ApiError::internal())?.len();
                if items.len()>=50||bytes+length>6*1024*1024 {more=true;break;}
                bytes+=length;cursor=format!("{}:{}",item.item.kind,item.item.id);items.push(item);
            }
            let head:i64=conn.query_row("SELECT next_seq FROM campaigns WHERE id=?1",[campaign],|r|r.get(0))?;
            Ok(ContentList{items,cursor,more,head:head.to_string()})
        })
    }
    /// Authorized current state of every item changed since `cursor`. The cursor only
    /// advances past rows already scanned, so a bounded page is resumable. Cursors that
    /// predate compacted rows, or are ahead of this server (restored older backup), are
    /// refused with 410 so the client resynchronizes from the full listing.
    pub fn sync(&self, token: &str, campaign: &str, cursor: &str) -> Result<SyncPage> {
        if cursor.len() > 20 {
            return Err(ApiError::invalid());
        }
        let since = if cursor.is_empty() {
            0
        } else {
            cursor.parse::<i64>().map_err(|_| ApiError::invalid())?
        };
        if since < 0 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            member(conn, campaign, &identity.account_id)?;
            let (head, floor): (i64, i64) = conn.query_row(
                "SELECT next_seq,log_floor FROM campaigns WHERE id=?1",
                [campaign],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if since < floor || since > head {
                return Err(ApiError(
                    StatusCode::GONE,
                    "Sync cursor is no longer valid. List the content and resume from its head.",
                ));
            }
            let mut q = conn.prepare("SELECT l.seq,l.kind,l.id FROM content_log l WHERE l.campaign_id=?1 AND l.seq>?2 AND (l.author_id=?3 OR l.state IN ('members','deleted','withdrawn')) ORDER BY l.seq LIMIT ?4")?;
            let rows = q
                .query_map(params![campaign, since, identity.account_id, SYNC_ROWS + 1], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut items: Vec<ContentItem> = Vec::new();
            let mut bytes = 0;
            let mut next = since;
            let mut more = false;
            let mut seen = std::collections::HashSet::new();
            for (index, (seq, kind_name, id)) in rows.iter().enumerate() {
                if index as i64 >= SYNC_ROWS {
                    more = true;
                    break;
                }
                if seen.insert((kind_name.clone(), id.clone()))
                    && let Some(item) =
                        visible_state(conn, campaign, &identity.account_id, kind_name, id)?
                {
                    let length = serde_json::to_vec(&item)
                        .map_err(|_| ApiError::internal())?
                        .len();
                    if !items.is_empty() && bytes + length > SYNC_BYTES {
                        more = true;
                        break;
                    }
                    bytes += length;
                    items.push(item);
                }
                next = *seq;
            }
            // Hidden rows were scanned too; when exhausted, jump to the head to skip them.
            if !more {
                next = head;
            }
            Ok(SyncPage { items, cursor: next.to_string(), more })
        })
    }
    /// Drops the oldest log rows beyond `keep` and raises the floor so stale cursors fail closed.
    pub fn compact_log(&self, campaign: &str, keep: i64) -> Result<u64> {
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let threshold: Option<i64> = tx
                .query_row(
                    "SELECT seq FROM content_log WHERE campaign_id=?1 ORDER BY seq DESC LIMIT 1 OFFSET ?2",
                    params![campaign, keep],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(threshold) = threshold else {
                return Ok(0);
            };
            let removed = tx.execute(
                "DELETE FROM content_log WHERE campaign_id=?1 AND seq<=?2",
                params![campaign, threshold],
            )?;
            tx.execute(
                "UPDATE campaigns SET log_floor=max(log_floor,?2) WHERE id=?1",
                params![campaign, threshold],
            )?;
            tx.commit()?;
            Ok(removed as u64)
        })
    }
    pub fn changes(&self, token: &str, campaign: &str, since: &str) -> Result<Changes> {
        if since.len() > 128 {
            return Err(ApiError::invalid());
        }
        let cursor = if since.is_empty() {
            0
        } else {
            since.parse::<i64>().map_err(|_| ApiError::invalid())?
        };
        if cursor < 0 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            member(conn,campaign,&identity.account_id)?;
            let max:i64=conn.query_row("SELECT next_seq FROM campaigns WHERE id=?1",[campaign],|r|r.get(0))?;if cursor>max{return Err(ApiError::invalid());}
            let mut q=conn.prepare("SELECT p.kind,p.id,c.author_id,a.display_name,p.revision,p.deleted,CASE WHEN p.deleted=0 AND c.visibility='members' AND c.deleted=0 THEN c.payload ELSE NULL END,p.updated_at,p.seq FROM published_items p JOIN content c ON c.campaign_id=p.campaign_id AND c.kind=p.kind AND c.id=p.id JOIN accounts a ON a.id=c.author_id WHERE p.campaign_id=?1 AND p.seq>?2 ORDER BY p.seq LIMIT 1001")?;
            let mut rows=q.query(params![campaign,cursor])?;let mut items=Vec::new();let mut next=cursor;let mut bytes=0;let mut more=false;
            while let Some(row)=rows.next()? {
                let raw:Option<String>=row.get(6)?;
                let item=Item{kind:row.get(0)?,id:row.get(1)?,author_id:row.get(2)?,author_name:row.get(3)?,revision:row.get(4)?,deleted:row.get(5)?,payload:raw.map(|p|serde_json::from_str(&p).map_err(|_|ApiError::internal())).transpose()?,updated_at:timestamp(row.get(7)?)};
                let length=serde_json::to_vec(&item).map_err(|_|ApiError::internal())?.len();if items.len()>=1000||bytes+length>6*1024*1024{more=true;break;}
                bytes+=length;next=row.get(8)?;items.push(item);
            }
            Ok(Changes{items,cursor:next.to_string(),more})
        })
    }
}
/// The current state of an item as `account` may see it, or None when it does not exist.
fn visible_state(
    conn: &Connection,
    campaign: &str,
    account: &str,
    kind_name: &str,
    id: &str,
) -> Result<Option<ContentItem>> {
    let Some(row) = stored(conn, campaign, kind_name, id)? else {
        return Ok(None);
    };
    let (name, time): (String, i64) = conn.query_row(
        "SELECT a.display_name,c.updated_at FROM content c JOIN accounts a ON a.id=c.author_id WHERE c.campaign_id=?1 AND c.kind=?2 AND c.id=?3",
        params![campaign, kind_name, id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let own = row.author == account;
    let live = !row.deleted && (own || row.visibility == Visibility::Members);
    // Other members only learn that a previously shared item is gone, never its private state.
    Ok(Some(ContentItem {
        visibility: if own {
            row.visibility
        } else {
            Visibility::Members
        },
        item: Item {
            kind: kind_name.into(),
            id: id.into(),
            author_id: row.author,
            author_name: name,
            revision: row.revision,
            deleted: !live,
            payload: if live { row.payload } else { None },
            updated_at: timestamp(time),
        },
    }))
}
fn content_row(row: &rusqlite::Row<'_>) -> Result<ContentItem> {
    let raw: String = row.get(5)?;
    let visibility: String = row.get(7)?;
    Ok(ContentItem {
        visibility: if visibility == "members" {
            Visibility::Members
        } else {
            Visibility::Private
        },
        item: Item {
            kind: row.get(0)?,
            id: row.get(1)?,
            author_id: row.get(2)?,
            author_name: row.get(3)?,
            revision: row.get(4)?,
            deleted: false,
            payload: Some(serde_json::from_str(&raw).map_err(|_| ApiError::internal())?),
            updated_at: timestamp(row.get(6)?),
        },
    })
}
