//! M25.11: browser interface and the read-only overviews it needs.
//!
//! The web pages are static files served from the binary. They sign in through the existing
//! JSON API and keep the Bearer token in the tab's session storage: no cookies, so no ambient
//! credential to forge (CSRF) and the same permission checks as every client. Every action the
//! pages offer (roles, removals, invitations, leaving, deletion, devices) calls the existing
//! endpoints; this module adds only read-only summaries. The operator role comes from
//! `CHRONEXUS_OPERATORS` and grants service status and account metadata, never campaign data.
use crate::{
    AppState,
    campaigns::{member, timestamp},
    db::Database,
    error::{ApiError, ErrorBody, Result},
    routes::token,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use rusqlite::params;
use serde::Serialize;
use std::collections::BTreeMap;
use utoipa::{OpenApi, ToSchema};

const INDEX: &str = include_str!("../web/index.html");
const SCRIPT: &str = include_str!("../web/app.js");
const TRANSLATIONS: &str = include_str!("../web/i18n.js");
const STYLE: &str = include_str!("../web/app.css");
/// Only the page's own files; no inline code, no third-party origin, no framing.
const PAGE_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// Limits the server enforces, shown so users understand a refused save.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Quotas {
    campaign_items: i64,
    campaign_content_bytes: i64,
    campaign_attachments: i64,
    campaign_attachment_bytes: i64,
    account_attachment_bytes: i64,
}
const QUOTAS: Quotas = Quotas {
    campaign_items: 5000,
    campaign_content_bytes: 64 * 1024 * 1024,
    campaign_attachments: 512,
    campaign_attachment_bytes: 128 * 1024 * 1024,
    account_attachment_bytes: 256 * 1024 * 1024,
};

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    account_id: String,
    username: String,
    display_name: String,
    created_at: String,
    /// Listed in CHRONEXUS_OPERATORS: may read service status and account metadata.
    operator: bool,
    gm_enabled: bool,
}
#[derive(Serialize, ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct KindUsage {
    kind: String,
    private_items: i64,
    shared_items: i64,
    bytes: i64,
    highest_revision: i64,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CampaignOverview {
    id: String,
    name: String,
    role: String,
    created_at: String,
    members: i64,
    /// What this account stored in the campaign, per item kind.
    my_items: Vec<KindUsage>,
    my_content_bytes: i64,
    my_attachments: i64,
    my_attachment_bytes: i64,
    /// All this account's uploads on the server (the per-account limit).
    account_attachment_bytes: i64,
    /// Latest save the server accepted from this account in this campaign.
    my_last_saved_at: Option<String>,
    /// Retry receipts kept for this account's operations in this campaign.
    my_recorded_operations: i64,
    /// Items other members currently share with everyone.
    shared_by_others: i64,
    quotas: Quotas,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorStatus {
    server_version: &'static str,
    schema: i64,
    database_bytes: i64,
    accounts: i64,
    disabled_accounts: i64,
    deleted_accounts: i64,
    active_sessions: i64,
    campaigns: i64,
    deleted_campaigns: i64,
    content_items: i64,
    content_bytes: i64,
    attachments: i64,
    attachment_bytes: i64,
    /// Server AI (M25.10): "configured" when the operator set a runtime, else "not installed".
    ai_service: &'static str,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperatorAccount {
    username: String,
    display_name: String,
    created_at: String,
    disabled: bool,
    deleted: bool,
    active_sessions: i64,
    campaigns: i64,
    stored_bytes: i64,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(|| async { page(INDEX, "text/html; charset=utf-8") }),
        )
        .route(
            "/web/app.js",
            get(|| async { page(SCRIPT, "text/javascript; charset=utf-8") }),
        )
        .route(
            "/web/i18n.js",
            get(|| async { page(TRANSLATIONS, "text/javascript; charset=utf-8") }),
        )
        .route(
            "/web/app.css",
            get(|| async { page(STYLE, "text/css; charset=utf-8") }),
        )
        .route("/api/v1/account", get(account))
        .route("/api/v1/campaigns/{campaignId}/overview", get(overview))
        .route("/api/v1/operator/status", get(operator_status))
        .route("/api/v1/operator/accounts", get(operator_accounts))
}
fn page(body: &'static str, kind: &'static str) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(PAGE_POLICY),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response
}

fn operator_check(db: &Database, token: &str, operators: &[String]) -> Result<()> {
    let identity = db.identity(token)?;
    let username: String = db.with(|conn| {
        Ok(conn.query_row(
            "SELECT username FROM accounts WHERE id=?1",
            [&identity.account_id],
            |r| r.get(0),
        )?)
    })?;
    if operators.contains(&username) {
        Ok(())
    } else {
        Err(ApiError(
            StatusCode::FORBIDDEN,
            "Operator role required for this operation.",
        ))
    }
}

impl Database {
    pub fn account(&self, token: &str, operators: &[String]) -> Result<Account> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            let (username, display_name, created, gm_enabled): (String, String, i64, bool) = conn
                .query_row(
                "SELECT username,display_name,created_at,gm_enabled FROM accounts WHERE id=?1",
                [&identity.account_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
            Ok(Account {
                operator: operators.contains(&username),
                gm_enabled,
                account_id: identity.account_id.clone(),
                username,
                display_name,
                created_at: timestamp(created),
            })
        })
    }
    /// Only this account's own usage plus counts every member can already see: the
    /// campaign-wide totals would reveal how much other members keep private.
    pub fn campaign_overview(&self, token: &str, campaign: &str) -> Result<CampaignOverview> {
        let identity = self.identity(token)?;
        let me = identity.account_id;
        self.with(|conn| {
            let role = member(conn, campaign, &me)?;
            let (name, created): (String, i64) = conn.query_row(
                "SELECT name,created_at FROM campaigns WHERE id=?1",
                [campaign],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let members: i64 = conn.query_row(
                "SELECT count(*) FROM members WHERE campaign_id=?1 AND active=1",
                [campaign],
                |r| r.get(0),
            )?;
            let mut my_items: BTreeMap<String, KindUsage> = BTreeMap::new();
            let mut my_content_bytes = 0;
            let mut last_saved: Option<i64> = None;
            let mut query = conn.prepare(
                "SELECT kind,visibility,count(*),coalesce(sum(length(CAST(payload AS BLOB))),0),max(revision),max(updated_at) \
                 FROM content WHERE campaign_id=?1 AND author_id=?2 AND deleted=0 GROUP BY kind,visibility",
            )?;
            let rows = query.query_map(params![campaign, me], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })?;
            for row in rows {
                let (kind, visibility, count, bytes, revision, updated) = row?;
                let usage = my_items.entry(kind.clone()).or_insert_with(|| KindUsage {
                    kind,
                    ..KindUsage::default()
                });
                if visibility == "members" {
                    usage.shared_items += count;
                } else {
                    usage.private_items += count;
                }
                usage.bytes += bytes;
                usage.highest_revision = usage.highest_revision.max(revision);
                my_content_bytes += bytes;
                last_saved = Some(last_saved.map_or(updated, |t: i64| t.max(updated)));
            }
            let (my_attachments, my_attachment_bytes): (i64, i64) = conn.query_row(
                "SELECT count(*),coalesce(sum(byte_length),0) FROM attachments WHERE campaign_id=?1 AND author_id=?2",
                params![campaign, me],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let account_attachment_bytes: i64 = conn.query_row(
                "SELECT coalesce(sum(byte_length),0) FROM attachments WHERE author_id=?1",
                [&me],
                |r| r.get(0),
            )?;
            let my_recorded_operations: i64 = conn.query_row(
                "SELECT count(*) FROM save_receipts WHERE campaign_id=?1 AND account_id=?2",
                params![campaign, me],
                |r| r.get(0),
            )?;
            let shared_by_others: i64 = conn.query_row(
                "SELECT count(*) FROM content WHERE campaign_id=?1 AND author_id<>?2 AND deleted=0 AND visibility='members'",
                params![campaign, me],
                |r| r.get(0),
            )?;
            Ok(CampaignOverview {
                id: campaign.into(),
                name,
                role,
                created_at: timestamp(created),
                members,
                my_items: my_items.into_values().collect(),
                my_content_bytes,
                my_attachments,
                my_attachment_bytes,
                account_attachment_bytes,
                my_last_saved_at: last_saved.map(timestamp),
                my_recorded_operations,
                shared_by_others,
                quotas: QUOTAS,
            })
        })
    }
    pub fn operator_status(
        &self,
        token: &str,
        operators: &[String],
        ai_service: &'static str,
    ) -> Result<OperatorStatus> {
        operator_check(self, token, operators)?;
        self.with(|conn| {
            let one = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
            Ok(OperatorStatus {
                server_version: env!("CARGO_PKG_VERSION"),
                schema: conn.pragma_query_value(None, "user_version", |r| r.get(0))?,
                database_bytes: one("SELECT page_count*page_size FROM pragma_page_count(),pragma_page_size()")?,
                accounts: one("SELECT count(*) FROM accounts WHERE deleted_at IS NULL")?,
                disabled_accounts: one("SELECT count(*) FROM accounts WHERE disabled=1 AND deleted_at IS NULL")?,
                deleted_accounts: one("SELECT count(*) FROM accounts WHERE deleted_at IS NOT NULL")?,
                active_sessions: conn.query_row(
                    "SELECT count(*) FROM sessions WHERE revoked_at IS NULL AND expires_at>?1 AND absolute_expires_at>?1",
                    [crate::auth::now()],
                    |r| r.get(0),
                )?,
                campaigns: one("SELECT count(*) FROM campaigns WHERE deleted_at IS NULL")?,
                deleted_campaigns: one("SELECT count(*) FROM campaigns WHERE deleted_at IS NOT NULL")?,
                content_items: one("SELECT count(*) FROM content WHERE deleted=0")?,
                content_bytes: one("SELECT coalesce(sum(length(CAST(payload AS BLOB))),0) FROM content WHERE deleted=0")?,
                attachments: one("SELECT count(*) FROM attachments")?,
                attachment_bytes: one("SELECT coalesce(sum(byte_length),0) FROM attachments")?,
                ai_service,
            })
        })
    }
    /// Account metadata only: no campaign names, members or content.
    pub fn operator_accounts(
        &self,
        token: &str,
        operators: &[String],
    ) -> Result<Vec<OperatorAccount>> {
        operator_check(self, token, operators)?;
        self.with(|conn| {
            let mut query = conn.prepare(
                "SELECT a.username,a.display_name,a.created_at,a.disabled,a.deleted_at IS NOT NULL,\
                 (SELECT count(*) FROM sessions s WHERE s.account_id=a.id AND s.revoked_at IS NULL AND s.expires_at>?1 AND s.absolute_expires_at>?1),\
                 (SELECT count(*) FROM members m JOIN campaigns c ON c.id=m.campaign_id WHERE m.account_id=a.id AND m.active=1 AND c.deleted_at IS NULL),\
                 (SELECT coalesce(sum(length(CAST(payload AS BLOB))),0) FROM content t WHERE t.author_id=a.id AND t.deleted=0)\
                 +(SELECT coalesce(sum(byte_length),0) FROM attachments x WHERE x.author_id=a.id) \
                 FROM accounts a ORDER BY a.created_at,a.id LIMIT 1000",
            )?;
            let rows = query.query_map([crate::auth::now()], |r| {
                Ok(OperatorAccount {
                    username: r.get(0)?,
                    display_name: r.get(1)?,
                    created_at: timestamp(r.get(2)?),
                    disabled: r.get(3)?,
                    deleted: r.get(4)?,
                    active_sessions: r.get(5)?,
                    campaigns: r.get(6)?,
                    stored_bytes: r.get(7)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }
}

#[utoipa::path(get,path="/api/v1/account",security(("bearer"=[])),responses((status=200,body=Account),(status=401,body=ErrorBody)),tag="accounts")]
async fn account(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Account>> {
    let token = token(&headers)?;
    let operators = state.config.operators.clone();
    state
        .blocking(move |db| db.account(&token, &operators))
        .await
        .map(Json)
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}/overview",params(("campaignId"=String,Path)),security(("bearer"=[])),responses((status=200,body=CampaignOverview),(status=403,body=ErrorBody),(status=404,body=ErrorBody)),tag="campaigns")]
async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
) -> Result<Json<CampaignOverview>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.campaign_overview(&token, &campaign))
        .await
        .map(Json)
}
#[utoipa::path(get,path="/api/v1/operator/status",security(("bearer"=[])),responses((status=200,body=OperatorStatus),(status=403,body=ErrorBody)),tag="operator")]
async fn operator_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OperatorStatus>> {
    let token = token(&headers)?;
    let operators = state.config.operators.clone();
    let ai = if state.ai.is_some() {
        "configured"
    } else {
        "not installed"
    };
    state
        .blocking(move |db| db.operator_status(&token, &operators, ai))
        .await
        .map(Json)
}
#[utoipa::path(get,path="/api/v1/operator/accounts",security(("bearer"=[])),responses((status=200,body=Vec<OperatorAccount>),(status=403,body=ErrorBody)),tag="operator")]
async fn operator_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<OperatorAccount>>> {
    let token = token(&headers)?;
    let operators = state.config.operators.clone();
    state
        .blocking(move |db| db.operator_accounts(&token, &operators))
        .await
        .map(Json)
}

#[derive(OpenApi)]
#[openapi(
    paths(account, overview, operator_status, operator_accounts),
    components(schemas(
        Account,
        CampaignOverview,
        KindUsage,
        Quotas,
        OperatorStatus,
        OperatorAccount
    ))
)]
pub(crate) struct WebContract;
