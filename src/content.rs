use crate::{
    auth::now,
    campaigns::{forbidden, identifier, member, missing, owner, timestamp},
    db::Database,
    error::{ApiError, Result},
    payload::Payload,
};
use axum::http::StatusCode;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const LEGACY: &[&str] = &["page", "session", "relationship", "party"];
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
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteItem {
    pub base_revision: i64,
    pub payload: Payload,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteContent {
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
    Ok(revision)
}
fn withdraw_dependents(conn: &Connection, campaign: &str, page: &str) -> Result<()> {
    let mut q=conn.prepare("SELECT c.kind,c.id FROM content c JOIN content_dependencies d ON d.campaign_id=c.campaign_id AND d.kind=c.kind AND d.item_id=c.id WHERE c.campaign_id=?1 AND d.page_id=?2 AND c.visibility='members' AND c.deleted=0")?;
    let rows = q
        .query_map(params![campaign, page], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (kind, id) in rows {
        withdraw(conn, campaign, &kind, &id)?;
    }
    Ok(())
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
        }
    }
    Ok(())
}
fn bump(revision: i64) -> Result<i64> {
    revision.checked_add(1).ok_or_else(ApiError::internal)
}
impl Database {
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
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;writable(&tx,campaign,&identity.account_id)?;
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
            tx.commit()?;Ok(Written::Accepted(Revision{revision}))
        })
    }
    pub fn delete_content(
        &self,
        token: &str,
        campaign: &str,
        kind_name: &str,
        id: &str,
        base: i64,
        legacy: bool,
    ) -> Result<Written> {
        identifier(id)?;
        kind(kind_name)?;
        if base < 1 || (legacy && !LEGACY.contains(&kind_name)) {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;writable(&tx,campaign,&identity.account_id)?;
            let old=stored(&tx,campaign,kind_name,id)?.ok_or_else(missing)?;
            if old.author!=identity.account_id {return Err(if old.visibility==Visibility::Private{missing()}else{forbidden()});}
            if old.deleted || (legacy&&old.visibility==Visibility::Private){return Err(missing());}
            if old.revision!=base{return Ok(Written::Conflict(Current{revision:old.revision,payload:old.payload}));}
            let revision=bump(base)?;
            if legacy {
                let revision=withdraw(&tx,campaign,kind_name,id)?;
                if kind_name=="page"{withdraw_dependents(&tx,campaign,id)?;}
                tx.commit()?;return Ok(Written::Accepted(Revision{revision}));
            }
            tx.execute("UPDATE content SET deleted=1,payload=NULL,revision=?1,updated_at=?2 WHERE campaign_id=?3 AND kind=?4 AND id=?5",params![revision,now(),campaign,kind_name,id])?;
            for table in ["content_dependencies","content_attachments"] {tx.execute(&format!("DELETE FROM {table} WHERE campaign_id=?1 AND kind=?2 AND item_id=?3"),params![campaign,kind_name,id])?;}
            if old.visibility==Visibility::Members{publish(&tx,campaign,kind_name,id,revision,true)?;}
            if kind_name=="page"{withdraw_dependents(&tx,campaign,id)?;}
            tx.commit()?;Ok(Written::Accepted(Revision{revision}))
        })
    }
    pub fn unshare_content(
        &self,
        token: &str,
        campaign: &str,
        kind_name: &str,
        id: &str,
        base: i64,
    ) -> Result<Written> {
        identifier(id)?;
        kind(kind_name)?;
        if base < 1 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            member(&tx, campaign, &identity.account_id)?;
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
            }
            tx.commit()?;
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
            Ok(ContentList{items,cursor,more})
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
