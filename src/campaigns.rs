//! Campaign access is checked against current membership inside each transaction.
use crate::{
    auth::{digest, now, random_secret},
    db::Database,
    error::{ApiError, Result},
};
use axum::http::StatusCode;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCampaign {
    pub name: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinCampaign {
    pub invite_code: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InviteRole {
    pub role: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeRole {
    pub role: String,
    pub base_revision: i64,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionQuery {
    pub base_revision: i64,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCampaign {
    pub id: String,
    pub name: String,
    pub role: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CampaignInvite {
    pub id: String,
    pub code: String,
    pub role: String,
    pub expires_at: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InviteStatus {
    pub id: String,
    pub role: String,
    pub expires_at: String,
    pub status: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub account_id: String,
    pub display_name: String,
    pub role: String,
    pub revision: i64,
    pub joined_at: String,
}

pub(crate) fn timestamp(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
pub(crate) fn identifier(value: &str) -> Result<()> {
    if Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value) {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}
pub(crate) fn text(value: &str, min: usize, max: usize) -> Result<()> {
    if (min..=max).contains(&value.chars().count())
        && value.trim() == value
        && !value.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}
pub(crate) fn missing() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "Resource not found.")
}
pub(crate) fn forbidden() -> ApiError {
    ApiError(
        StatusCode::FORBIDDEN,
        "Campaign access does not permit this operation.",
    )
}
fn role(value: &str) -> Result<()> {
    if ["player", "reader"].contains(&value) {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}
pub(crate) fn member(conn: &Connection, campaign: &str, account: &str) -> Result<String> {
    identifier(campaign)?;
    let row:Option<(String,bool,Option<i64>)>=conn.query_row("SELECT m.role,m.active,c.deleted_at FROM members m JOIN accounts a ON a.id=m.account_id JOIN campaigns c ON c.id=m.campaign_id WHERE m.campaign_id=?1 AND m.account_id=?2 AND a.disabled=0",params![campaign,account],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    // Former members of a deleted campaign learn that it is gone; everyone else sees a refusal.
    match row {
        Some((role, true, None)) => Ok(role),
        Some((_, _, Some(_))) => Err(crate::lifecycle::campaign_gone()),
        _ => Err(forbidden()),
    }
}
pub(crate) fn owner(conn: &Connection, campaign: &str, account: &str) -> Result<()> {
    if member(conn, campaign, account)? == "owner" {
        Ok(())
    } else {
        Err(forbidden())
    }
}
fn membership_quota(conn: &Connection, account: &str) -> Result<()> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM members WHERE account_id=?1 AND active=1",
        [account],
        |r| r.get(0),
    )?;
    if count >= 100 {
        Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Campaign membership quota reached.",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn available_invite(conn: &Connection, hash: &str) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM campaign_invites i JOIN campaigns c ON c.id=i.campaign_id JOIN accounts a ON a.id=i.created_by JOIN members m ON m.campaign_id=c.id AND m.account_id=a.id WHERE i.code_hash=?1 AND i.used_at IS NULL AND i.revoked_at IS NULL AND i.expires_at>?2 AND c.deleted_at IS NULL AND c.owner_id=i.created_by AND m.role='owner' AND m.active=1 AND a.disabled=0)",params![hash,now()],|r|r.get(0))?)
}
pub(crate) fn accept_invite(
    conn: &Connection,
    hash: &str,
    account: &str,
) -> Result<RemoteCampaign> {
    if !available_invite(conn, hash)? {
        return Err(ApiError::gone());
    }
    let (id,name,role):(String,String,String)=conn.query_row("SELECT c.id,c.name,i.role FROM campaign_invites i JOIN campaigns c ON c.id=i.campaign_id WHERE i.code_hash=?1",[hash],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM members WHERE campaign_id=?1 AND account_id=?2 AND active=1)",
        params![id, account],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Account is already a member.",
        ));
    }
    membership_quota(conn, account)?;
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM members WHERE campaign_id=?1 AND active=1",
        [&id],
        |r| r.get(0),
    )?;
    if count >= 100 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Campaign member quota reached.",
        ));
    }
    conn.execute("INSERT INTO members(campaign_id,account_id,role,joined_at) VALUES(?1,?2,?3,?4) ON CONFLICT(campaign_id,account_id) DO UPDATE SET active=1,role=excluded.role,joined_at=excluded.joined_at,revision=members.revision+1",params![id,account,role,now()])?;
    conn.execute(
        "UPDATE campaign_invites SET used_at=?1,used_by=?2 WHERE code_hash=?3",
        params![now(), account, hash],
    )?;
    Ok(RemoteCampaign { id, name, role })
}

impl Database {
    pub fn create_campaign(&self, token: &str, input: &CreateCampaign) -> Result<RemoteCampaign> {
        text(&input.name, 1, 200)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            membership_quota(&tx,&identity.account_id)?;
            let id=Uuid::new_v4().to_string();
            tx.execute("INSERT INTO campaigns(id,name,owner_id,created_at) VALUES(?1,?2,?3,?4)",params![id,input.name,identity.account_id,now()])?;
            tx.execute("INSERT INTO members(campaign_id,account_id,role,joined_at) VALUES(?1,?2,'owner',?3)",params![id,identity.account_id,now()])?;
            tx.commit()?;Ok(RemoteCampaign{id,name:input.name.clone(),role:"owner".into()})
        })
    }
    pub fn campaigns(&self, token: &str) -> Result<Vec<RemoteCampaign>> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            let mut query=conn.prepare("SELECT c.id,c.name,m.role FROM campaigns c JOIN members m ON m.campaign_id=c.id WHERE m.account_id=?1 AND m.active=1 ORDER BY c.created_at,c.id LIMIT 100")?;
            Ok(query.query_map([identity.account_id],|r|Ok(RemoteCampaign{id:r.get(0)?,name:r.get(1)?,role:r.get(2)?}))?.collect::<std::result::Result<Vec<_>,_>>()?)
        })
    }
    pub fn campaign(&self, token: &str, id: &str) -> Result<RemoteCampaign> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            let role = member(conn, id, &identity.account_id)?;
            let name =
                conn.query_row("SELECT name FROM campaigns WHERE id=?1", [id], |r| r.get(0))?;
            Ok(RemoteCampaign {
                id: id.into(),
                name,
                role,
            })
        })
    }
    pub fn campaign_invite(
        &self,
        token: &str,
        campaign: &str,
        input: &InviteRole,
    ) -> Result<CampaignInvite> {
        role(&input.role)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            owner(&tx,campaign,&identity.account_id)?;
            tx.execute("DELETE FROM campaign_invites WHERE expires_at<?1",[now()-30*86400])?;
            let (pending,total):(i64,i64)=tx.query_row("SELECT sum(CASE WHEN used_at IS NULL AND revoked_at IS NULL AND expires_at>?2 THEN 1 ELSE 0 END),count(*) FROM campaign_invites WHERE campaign_id=?1",params![campaign,now()],|r|Ok((r.get::<_,Option<i64>>(0)?.unwrap_or(0),r.get(1)?)))?;
            if pending>=100 || total>=1000 {return Err(ApiError(StatusCode::TOO_MANY_REQUESTS,"Invitation quota reached."));}
            let code=random_secret()?;let id=Uuid::new_v4().to_string();let expires=now()+7*86400;
            tx.execute("INSERT INTO campaign_invites(id,code_hash,campaign_id,role,created_by,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,digest(&code),campaign,input.role,identity.account_id,now(),expires])?;
            tx.commit()?;Ok(CampaignInvite{id,code,role:input.role.clone(),expires_at:timestamp(expires)})
        })
    }
    pub fn campaign_invites(&self, token: &str, campaign: &str) -> Result<Vec<InviteStatus>> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            owner(conn,campaign,&identity.account_id)?;
            let mut query=conn.prepare("SELECT id,role,expires_at,CASE WHEN revoked_at IS NOT NULL THEN 'revoked' WHEN used_at IS NOT NULL THEN 'used' WHEN expires_at<=?2 THEN 'expired' ELSE 'pending' END FROM campaign_invites WHERE campaign_id=?1 ORDER BY created_at DESC,id LIMIT 1000")?;
            Ok(query.query_map(params![campaign,now()],|r|Ok(InviteStatus{id:r.get(0)?,role:r.get(1)?,expires_at:timestamp(r.get(2)?),status:r.get(3)?}))?.collect::<std::result::Result<Vec<_>,_>>()?)
        })
    }
    pub fn revoke_campaign_invite(&self, token: &str, campaign: &str, id: &str) -> Result<()> {
        identifier(id)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;owner(&tx,campaign,&identity.account_id)?;
            if tx.execute("UPDATE campaign_invites SET revoked_at=?1 WHERE id=?2 AND campaign_id=?3 AND revoked_at IS NULL AND used_at IS NULL",params![now(),id,campaign])?==0 {return Err(missing());}
            tx.commit()?;Ok(())
        })
    }
    pub fn join_campaign(
        &self,
        token: &str,
        input: &JoinCampaign,
        ip: &str,
    ) -> Result<RemoteCampaign> {
        if input.invite_code.len() != 64 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.attempt(ip, &digest(&input.invite_code), "campaign-join")?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let joined = accept_invite(&tx, &digest(&input.invite_code), &identity.account_id)?;
            tx.commit()?;
            Ok(joined)
        })
    }
    pub fn campaign_members(&self, token: &str, campaign: &str) -> Result<Vec<Member>> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            member(conn,campaign,&identity.account_id)?;
            let mut query=conn.prepare("SELECT m.account_id,a.display_name,m.role,m.revision,m.joined_at FROM members m JOIN accounts a ON a.id=m.account_id WHERE m.campaign_id=?1 AND m.active=1 ORDER BY m.joined_at,m.account_id LIMIT 100")?;
            Ok(query.query_map([campaign],|r|Ok(Member{account_id:r.get(0)?,display_name:r.get(1)?,role:r.get(2)?,revision:r.get(3)?,joined_at:timestamp(r.get(4)?)}))?.collect::<std::result::Result<Vec<_>,_>>()?)
        })
    }
    pub fn change_member(
        &self,
        token: &str,
        campaign: &str,
        account: &str,
        input: &ChangeRole,
    ) -> Result<()> {
        role(&input.role)?;
        identifier(account)?;
        if input.base_revision < 1 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;owner(&tx,campaign,&identity.account_id)?;
            let (role,revision)=target(&tx,campaign,account)?;
            if role=="owner" {return Err(forbidden());}
            if revision!=input.base_revision {return Err(ApiError(StatusCode::CONFLICT,"Membership changed; reload before editing."));}
            tx.execute("UPDATE members SET role=?1,revision=revision+1 WHERE campaign_id=?2 AND account_id=?3",params![input.role,campaign,account])?;
            tx.commit()?;Ok(())
        })
    }
    pub fn remove_member(
        &self,
        token: &str,
        campaign: &str,
        account: &str,
        revision: i64,
    ) -> Result<()> {
        identifier(account)?;
        if revision < 1 {
            return Err(ApiError::invalid());
        }
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            member(&tx,campaign,&identity.account_id)?;
            if identity.account_id!=account {owner(&tx,campaign,&identity.account_id)?;}
            let (role,current)=target(&tx,campaign,account)?;if role=="owner" {return Err(forbidden());}
            if current!=revision {return Err(ApiError(StatusCode::CONFLICT,"Membership changed; reload before removal."));}
            crate::content::withdraw_author(&tx,campaign,account)?;
            tx.execute("UPDATE members SET active=0,revision=revision+1 WHERE campaign_id=?1 AND account_id=?2",params![campaign,account])?;
            tx.commit()?;Ok(())
        })
    }
}
fn target(conn: &Connection, campaign: &str, account: &str) -> Result<(String, i64)> {
    conn.query_row(
        "SELECT role,revision FROM members WHERE campaign_id=?1 AND account_id=?2 AND active=1",
        params![campaign, account],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(missing)
}
