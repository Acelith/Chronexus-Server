use crate::{
    AppState,
    campaigns::*,
    error::{ErrorBody, Result},
    routes::{body, token},
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
};
use utoipa::OpenApi;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/campaigns", post(create).get(list))
        .route("/api/v1/campaigns/join", post(join))
        .route("/api/v1/campaigns/{campaignId}", get(detail))
        .route(
            "/api/v1/campaigns/{campaignId}/invites",
            post(invite).get(invites),
        )
        .route(
            "/api/v1/campaigns/{campaignId}/invites/{inviteId}",
            delete(revoke_invite),
        )
        .route("/api/v1/campaigns/{campaignId}/members", get(members))
        .route(
            "/api/v1/campaigns/{campaignId}/members/{accountId}",
            delete(remove).patch(change),
        )
}
#[utoipa::path(post,path="/api/v1/campaigns",request_body=CreateCampaign,security(("bearer"=[])),responses((status=201,body=RemoteCampaign),(status=400,body=ErrorBody),(status=401,body=ErrorBody),(status=429,body=ErrorBody)),tag="campaigns")]
async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    input: std::result::Result<Json<CreateCampaign>, JsonRejection>,
) -> Result<(StatusCode, Json<RemoteCampaign>)> {
    let token = token(&headers)?;
    let input = body(input)?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .blocking(move |db| db.create_campaign(&token, &input))
                .await?,
        ),
    ))
}
#[utoipa::path(get,path="/api/v1/campaigns",security(("bearer"=[])),responses((status=200,body=Vec<RemoteCampaign>),(status=401,body=ErrorBody)),tag="campaigns")]
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RemoteCampaign>>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.campaigns(&token))
        .await
        .map(Json)
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}",params(("campaignId"=String,Path)),security(("bearer"=[])),responses((status=200,body=RemoteCampaign),(status=403,body=ErrorBody)),tag="campaigns")]
async fn detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
) -> Result<Json<RemoteCampaign>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.campaign(&token, &campaign))
        .await
        .map(Json)
}
#[utoipa::path(post,path="/api/v1/campaigns/join",request_body=JoinCampaign,security(("bearer"=[])),responses((status=200,body=RemoteCampaign),(status=409,body=ErrorBody),(status=410,body=ErrorBody),(status=429,body=ErrorBody)),tag="campaigns")]
async fn join(
    State(state): State<AppState>,
    axum::Extension(ip): axum::Extension<std::net::IpAddr>,
    headers: HeaderMap,
    input: std::result::Result<Json<JoinCampaign>, JsonRejection>,
) -> Result<Json<RemoteCampaign>> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.join_campaign(&token, &input, &ip.to_string()))
        .await
        .map(Json)
}
#[utoipa::path(post,path="/api/v1/campaigns/{campaignId}/invites",params(("campaignId"=String,Path)),request_body=InviteRole,security(("bearer"=[])),responses((status=201,body=CampaignInvite),(status=403,body=ErrorBody),(status=429,body=ErrorBody)),tag="campaigns")]
async fn invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
    input: std::result::Result<Json<InviteRole>, JsonRejection>,
) -> Result<(StatusCode, Json<CampaignInvite>)> {
    let token = token(&headers)?;
    let input = body(input)?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .blocking(move |db| db.campaign_invite(&token, &campaign, &input))
                .await?,
        ),
    ))
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}/invites",params(("campaignId"=String,Path)),security(("bearer"=[])),responses((status=200,body=Vec<InviteStatus>),(status=403,body=ErrorBody)),tag="campaigns")]
async fn invites(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
) -> Result<Json<Vec<InviteStatus>>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.campaign_invites(&token, &campaign))
        .await
        .map(Json)
}
#[utoipa::path(delete,path="/api/v1/campaigns/{campaignId}/invites/{inviteId}",params(("campaignId"=String,Path),("inviteId"=String,Path)),security(("bearer"=[])),responses((status=204),(status=403,body=ErrorBody),(status=404,body=ErrorBody)),tag="campaigns")]
async fn revoke_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.revoke_campaign_invite(&token, &campaign, &id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}/members",params(("campaignId"=String,Path)),security(("bearer"=[])),responses((status=200,body=Vec<Member>),(status=403,body=ErrorBody)),tag="campaigns")]
async fn members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
) -> Result<Json<Vec<Member>>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.campaign_members(&token, &campaign))
        .await
        .map(Json)
}
#[utoipa::path(patch,path="/api/v1/campaigns/{campaignId}/members/{accountId}",params(("campaignId"=String,Path),("accountId"=String,Path)),request_body=ChangeRole,security(("bearer"=[])),responses((status=204),(status=403,body=ErrorBody),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="campaigns")]
async fn change(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, account)): Path<(String, String)>,
    input: std::result::Result<Json<ChangeRole>, JsonRejection>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.change_member(&token, &campaign, &account, &input))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(delete,path="/api/v1/campaigns/{campaignId}/members/{accountId}",params(("campaignId"=String,Path),("accountId"=String,Path),("baseRevision"=i64,Query)),security(("bearer"=[])),responses((status=204),(status=403,body=ErrorBody),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="campaigns")]
async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, account)): Path<(String, String)>,
    query: std::result::Result<Query<RevisionQuery>, QueryRejection>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    let Query(query) = query.map_err(|_| crate::error::ApiError::invalid())?;
    state
        .blocking(move |db| db.remove_member(&token, &campaign, &account, query.base_revision))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(OpenApi)]
#[openapi(
    paths(
        create,
        list,
        detail,
        join,
        invite,
        invites,
        revoke_invite,
        members,
        change,
        remove
    ),
    components(schemas(
        CreateCampaign,
        JoinCampaign,
        InviteRole,
        ChangeRole,
        RevisionQuery,
        RemoteCampaign,
        CampaignInvite,
        InviteStatus,
        Member
    ))
)]
pub(crate) struct CampaignContract;
