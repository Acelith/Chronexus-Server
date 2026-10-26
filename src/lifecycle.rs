//! S6: revocation, deletion and recovery boundaries.
//!
//! * Removing a member or unsharing an item (S4) never touches the author's private original.
//! * An author may erase everything they synchronized to one campaign.
//! * An owner may delete a campaign; an account may delete itself.
//! * Operators may purge old tombstones and revoke every session after restoring a backup.
//!
//! Deletion removes payloads, uploads, change-log rows, receipts and invitations. What stays
//! is content-free: ids, revisions and the tombstones other members still need to learn that
//! something is gone. Nothing here can erase a copy a member already exported, or an offline
//! device that has not reconnected; backups are retained separately by the operator.
use crate::{
    AppState,
    auth::{digest, now, password_hash, verify},
    campaigns::{identifier, member, owner},
    content::{log_change, publish, withdraw_dependents},
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
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;
use zeroize::Zeroize;

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteCampaign {
    /// The exact campaign name: an explicit confirmation of a destructive action.
    pub confirm_name: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EraseOwnContent {
    /// Must be true: the server copy of everything you synchronized here is deleted.
    pub confirm: bool,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Erased {
    pub items: i64,
    pub attachments: i64,
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteAccount {
    pub password: String,
    /// Required when you own a campaign that other members still use: it is deleted too.
    #[serde(default)]
    pub delete_owned_campaigns: bool,
}
impl Drop for DeleteAccount {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

fn deleted_campaign_error() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "This campaign was deleted.")
}
pub(crate) fn campaign_gone() -> ApiError {
    deleted_campaign_error()
}

/// Tombstones everything `account` authored in `campaign` and removes their uploads.
/// Receipts are kept on purpose: a retried save must be acknowledged, never re-applied.
fn erase_own_content(tx: &Connection, campaign: &str, account: &str) -> Result<(i64, i64)> {
    let mut query = tx.prepare(
        "SELECT kind,id,visibility FROM content WHERE campaign_id=?1 AND author_id=?2 AND deleted=0",
    )?;
    let rows = query
        .query_map(params![campaign, account], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(query);
    let mut items = 0;
    for (kind, id, visibility) in rows {
        let public = visibility == "members";
        let revision: i64 = tx.query_row(
            "UPDATE content SET deleted=1,payload=NULL,visibility='private',revision=revision+1,updated_at=?1 WHERE campaign_id=?2 AND kind=?3 AND id=?4 RETURNING revision",
            params![now(), campaign, kind, id],
            |r| r.get(0),
        )?;
        for table in ["content_dependencies", "content_attachments"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE campaign_id=?1 AND kind=?2 AND item_id=?3"),
                params![campaign, kind, id],
            )?;
        }
        if public {
            publish(tx, campaign, &kind, &id, revision, true)?;
        }
        log_change(
            tx,
            campaign,
            &kind,
            &id,
            account,
            if public { "deleted" } else { "deleted-private" },
        )?;
        if kind == "page" {
            withdraw_dependents(tx, campaign, &id)?;
        }
        items += 1;
    }
    let attachments = tx.execute(
        "DELETE FROM attachments WHERE campaign_id=?1 AND author_id=?2",
        params![campaign, account],
    )? as i64;
    Ok((items, attachments))
}
/// Removes everything a campaign holds. The id, the deletion time and the inactive
/// membership rows remain so former members are told the campaign is gone.
fn purge_campaign(tx: &Connection, campaign: &str) -> Result<()> {
    for sql in [
        "DELETE FROM content_attachments WHERE campaign_id=?1",
        "DELETE FROM content_dependencies WHERE campaign_id=?1",
        "DELETE FROM published_items WHERE campaign_id=?1",
        "DELETE FROM content WHERE campaign_id=?1",
        "DELETE FROM attachments WHERE campaign_id=?1",
        "DELETE FROM content_log WHERE campaign_id=?1",
        "DELETE FROM save_receipts WHERE campaign_id=?1",
        "DELETE FROM campaign_invites WHERE campaign_id=?1",
    ] {
        tx.execute(sql, [campaign])?;
    }
    tx.execute(
        "UPDATE members SET active=0,revision=revision+1 WHERE campaign_id=?1",
        [campaign],
    )?;
    tx.execute(
        "UPDATE campaigns SET deleted_at=?2,name='Deleted campaign' WHERE id=?1",
        params![campaign, now()],
    )?;
    Ok(())
}

impl Database {
    /// Owner only, after typing the exact name. Irreversible on the server.
    pub fn delete_campaign(
        &self,
        token: &str,
        campaign: &str,
        input: &DeleteCampaign,
    ) -> Result<()> {
        identifier(campaign)?;
        let identity = self.identity(token)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            owner(&tx, campaign, &identity.account_id)?;
            let name: String =
                tx.query_row("SELECT name FROM campaigns WHERE id=?1", [campaign], |r| {
                    r.get(0)
                })?;
            if name != input.confirm_name {
                return Err(ApiError(
                    StatusCode::BAD_REQUEST,
                    "Type the exact campaign name to confirm.",
                ));
            }
            purge_campaign(&tx, campaign)?;
            tx.commit()?;
            Ok(())
        })?;
        self.notify(campaign, &identity.account_id, true);
        Ok(())
    }
    /// Any member: deletes the server copy of everything they synchronized here and stops.
    /// Other members' items are untouched; membership continues.
    pub fn erase_own_content(
        &self,
        token: &str,
        campaign: &str,
        input: &EraseOwnContent,
    ) -> Result<Erased> {
        identifier(campaign)?;
        let identity = self.identity(token)?;
        // Authorize first: a refusal must not depend on what the body says.
        self.with(|conn| member(conn, campaign, &identity.account_id).map(|_| ()))?;
        if !input.confirm {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "Confirm the deletion of your server copy.",
            ));
        }
        let erased = self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            member(&tx, campaign, &identity.account_id)?;
            let (items, attachments) = erase_own_content(&tx, campaign, &identity.account_id)?;
            tx.commit()?;
            Ok(Erased { items, attachments })
        })?;
        self.notify(campaign, &identity.account_id, true);
        Ok(erased)
    }
    /// Deletes the account after a password check. Campaigns it owns that other members
    /// still use need an explicit flag; the rest of its content is erased everywhere.
    pub fn delete_account(&self, token: &str, input: &DeleteAccount, ip: &str) -> Result<()> {
        let identity = self.identity(token)?;
        self.attempt(ip, &digest(&identity.account_id), "account-delete")?;
        let hash: String = self.with(|conn| {
            Ok(conn.query_row(
                "SELECT password_hash FROM accounts WHERE id=?1 AND disabled=0",
                [&identity.account_id],
                |r| r.get(0),
            )?)
        })?;
        if !verify(&input.password, &hash) {
            return Err(ApiError(
                StatusCode::UNAUTHORIZED,
                "The password is not correct.",
            ));
        }
        let touched = self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut query = tx.prepare(
                "SELECT m.campaign_id,m.role FROM members m JOIN campaigns c ON c.id=m.campaign_id WHERE m.account_id=?1 AND m.active=1 AND c.deleted_at IS NULL",
            )?;
            let memberships = query
                .query_map([&identity.account_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(query);
            // Check every owned campaign before changing anything.
            for (campaign, role) in &memberships {
                if role == "owner" && !input.delete_owned_campaigns {
                    let others: i64 = tx.query_row(
                        "SELECT count(*) FROM members WHERE campaign_id=?1 AND active=1 AND account_id<>?2",
                        params![campaign, identity.account_id],
                        |r| r.get(0),
                    )?;
                    if others > 0 {
                        return Err(ApiError(
                            StatusCode::CONFLICT,
                            "You own a campaign other members use. Delete it first, or confirm deleting it with your account.",
                        ));
                    }
                }
            }
            for (campaign, role) in &memberships {
                if role == "owner" {
                    purge_campaign(&tx, campaign)?;
                } else {
                    erase_own_content(&tx, campaign, &identity.account_id)?;
                    tx.execute(
                        "UPDATE members SET active=0,revision=revision+1 WHERE campaign_id=?1 AND account_id=?2",
                        params![campaign, identity.account_id],
                    )?;
                }
            }
            tx.execute(
                "UPDATE campaign_invites SET revoked_at=?1 WHERE created_by=?2 AND used_at IS NULL AND revoked_at IS NULL",
                params![now(), identity.account_id],
            )?;
            tx.execute(
                "UPDATE invites SET used_at=?1 WHERE recovery_account_id=?2 AND used_at IS NULL",
                params![now(), identity.account_id],
            )?;
            tx.execute(
                "UPDATE sessions SET revoked_at=?1 WHERE account_id=?2 AND revoked_at IS NULL",
                params![now(), identity.account_id],
            )?;
            // An anonymous stub keeps the content-free history that references the account.
            tx.execute(
                "UPDATE accounts SET username=?2,display_name='Deleted account',password_hash=?3,disabled=1,deleted_at=?4 WHERE id=?1",
                params![
                    identity.account_id,
                    format!("deleted-{}", Uuid::new_v4()),
                    password_hash(&Uuid::new_v4().to_string())?,
                    now()
                ],
            )?;
            tx.commit()?;
            Ok(memberships)
        })?;
        for (campaign, _) in touched {
            self.notify(&campaign, &identity.account_id, true);
        }
        Ok(())
    }
    /// Operator: invalidates every session, for example after restoring an older backup
    /// that still contains sessions revoked since. Everyone signs in again.
    pub fn revoke_all_sessions(&self) -> Result<u64> {
        self.with(|conn| {
            Ok(conn.execute(
                "UPDATE sessions SET revoked_at=?1 WHERE revoked_at IS NULL",
                [now()],
            )? as u64)
        })
    }
    /// Operator: drops tombstones older than `days`. A client whose cursor predates their
    /// log rows is made to list the campaign again (the floor rises), so a purged
    /// tombstone can never let a stale client resurrect the item.
    pub fn purge_tombstones(&self, days: i64) -> Result<u64> {
        if !(0..=3650).contains(&days) {
            return Err(ApiError::invalid());
        }
        let cutoff = now() - days * 86_400;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut query = tx.prepare(
                "SELECT DISTINCT campaign_id FROM content WHERE deleted=1 AND updated_at<=?1",
            )?;
            let campaigns = query
                .query_map([cutoff], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(query);
            let mut removed = 0;
            for campaign in campaigns {
                let threshold: Option<i64> = tx.query_row(
                    "SELECT max(l.seq) FROM content_log l JOIN content c ON c.campaign_id=l.campaign_id AND c.kind=l.kind AND c.id=l.id WHERE l.campaign_id=?1 AND c.deleted=1 AND c.updated_at<=?2",
                    params![campaign, cutoff],
                    |r| r.get(0),
                )?;
                tx.execute(
                    "DELETE FROM published_items WHERE campaign_id=?1 AND EXISTS(SELECT 1 FROM content c WHERE c.campaign_id=published_items.campaign_id AND c.kind=published_items.kind AND c.id=published_items.id AND c.deleted=1 AND c.updated_at<=?2)",
                    params![campaign, cutoff],
                )?;
                removed += tx.execute(
                    "DELETE FROM content WHERE campaign_id=?1 AND deleted=1 AND updated_at<=?2",
                    params![campaign, cutoff],
                )? as u64;
                if let Some(threshold) = threshold {
                    tx.execute(
                        "DELETE FROM content_log WHERE campaign_id=?1 AND seq<=?2",
                        params![campaign, threshold],
                    )?;
                    tx.execute(
                        "UPDATE campaigns SET log_floor=max(log_floor,?2) WHERE id=?1",
                        params![campaign, threshold],
                    )?;
                }
            }
            tx.commit()?;
            Ok(removed)
        })
    }
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/campaigns/{campaignId}/deletion",
            post(delete_campaign),
        )
        .route(
            "/api/v1/campaigns/{campaignId}/own-content/deletion",
            post(erase_own),
        )
        .route("/api/v1/auth/account/deletion", post(delete_account))
}
#[utoipa::path(post,path="/api/v1/campaigns/{campaignId}/deletion",params(("campaignId"=String,Path)),request_body=DeleteCampaign,security(("bearer"=[])),responses((status=204),(status=400,body=ErrorBody),(status=403,body=ErrorBody),(status=404,body=ErrorBody)),tag="lifecycle")]
async fn delete_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
    input: std::result::Result<Json<DeleteCampaign>, JsonRejection>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.delete_campaign(&token, &campaign, &input))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(post,path="/api/v1/campaigns/{campaignId}/own-content/deletion",params(("campaignId"=String,Path)),request_body=EraseOwnContent,security(("bearer"=[])),responses((status=200,body=Erased),(status=400,body=ErrorBody),(status=403,body=ErrorBody),(status=404,body=ErrorBody)),tag="lifecycle")]
async fn erase_own(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
    input: std::result::Result<Json<EraseOwnContent>, JsonRejection>,
) -> Result<Json<Erased>> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.erase_own_content(&token, &campaign, &input))
        .await
        .map(Json)
}
#[utoipa::path(post,path="/api/v1/auth/account/deletion",request_body=DeleteAccount,security(("bearer"=[])),responses((status=204),(status=401,body=ErrorBody),(status=409,body=ErrorBody),(status=429,body=ErrorBody)),tag="lifecycle")]
async fn delete_account(
    State(state): State<AppState>,
    axum::Extension(ip): axum::Extension<std::net::IpAddr>,
    headers: HeaderMap,
    input: std::result::Result<Json<DeleteAccount>, JsonRejection>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.delete_account(&token, &input, &ip.to_string()))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(OpenApi)]
#[openapi(
    paths(delete_campaign, erase_own, delete_account),
    components(schemas(DeleteCampaign, EraseOwnContent, Erased, DeleteAccount))
)]
pub(crate) struct LifecycleContract;
