use crate::{
    AppState,
    attachments::{Attachment, MAX_BYTES},
    campaigns::RevisionQuery,
    content::*,
    error::{ApiError, ErrorBody, Result},
    routes::{body, token},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, Path, Query, Request, State,
        rejection::{BytesRejection, JsonRejection, QueryRejection},
    },
    http::{HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use utoipa::OpenApi;

/// OpenAPI-only marker for a raw binary image body.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(value_type=String,format=Binary)]
pub struct BinaryImage(pub Vec<u8>);

pub(crate) fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/campaigns/{campaignId}/items/{kind}/{itemId}",
            put(legacy_put).delete(legacy_delete),
        )
        .route("/api/v1/campaigns/{campaignId}/changes", get(changes))
        .route("/api/v1/campaigns/{campaignId}/content", get(list))
        .route(
            "/api/v1/campaigns/{campaignId}/content/{kind}/{itemId}",
            get(detail).put(save).delete(remove),
        )
        .route(
            "/api/v1/campaigns/{campaignId}/content/{kind}/{itemId}/sharing",
            delete(unshare),
        )
        .route(
            "/api/v1/campaigns/{campaignId}/attachments",
            post(upload).layer(DefaultBodyLimit::max(MAX_BYTES)),
        )
        .route(
            "/api/v1/campaigns/{campaignId}/attachments/{attachmentId}",
            get(download).delete(remove_attachment),
        )
        .layer(DefaultBodyLimit::max(2_100_000))
        .layer(middleware::from_fn_with_state(state, admission))
}
async fn admission(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let credentials = token(request.headers());
    let campaign = request
        .uri()
        .path()
        .split('/')
        .nth(4)
        .map(String::from)
        .ok_or_else(ApiError::invalid);
    let write = ![Method::GET, Method::HEAD].contains(request.method());
    let access = async {
        let token = credentials?;
        let campaign = campaign?;
        let permit = if write {
            Some(state.transfers.clone().try_acquire_owned().map_err(|_| {
                ApiError(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Content transfer busy. Retry later.",
                )
            })?)
        } else {
            None
        };
        state
            .blocking(move |db| db.authorize_content(&token, &campaign, write))
            .await?;
        Ok::<_, ApiError>(permit)
    }
    .await;
    match access {
        Ok(_permit) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}
fn written(result: Written) -> Response {
    match result {
        Written::Accepted(r) => Json(r).into_response(),
        Written::Conflict(r) => (StatusCode::CONFLICT, Json(r)).into_response(),
    }
}
fn query<T>(query: std::result::Result<Query<T>, QueryRejection>) -> Result<T> {
    query.map(|Query(q)| q).map_err(|_| ApiError::invalid())
}

#[utoipa::path(put,path="/api/v1/campaigns/{campaignId}/items/{kind}/{itemId}",params(("campaignId"=String,Path),("kind"=String,Path),("itemId"=String,Path)),request_body=WriteItem,security(("bearer"=[])),responses((status=200,body=Revision),(status=409,body=Current),(status=403,body=ErrorBody)),tag="sharing")]
async fn legacy_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, kind, id)): Path<(String, String, String)>,
    input: std::result::Result<Json<WriteItem>, JsonRejection>,
) -> Result<Response> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| {
            db.write_content(
                &token,
                &campaign,
                &kind,
                &id,
                &WriteContent {
                    base_revision: input.base_revision,
                    payload: input.payload,
                    visibility: Visibility::Members,
                },
                true,
            )
        })
        .await
        .map(written)
}
#[utoipa::path(delete,path="/api/v1/campaigns/{campaignId}/items/{kind}/{itemId}",params(("campaignId"=String,Path),("kind"=String,Path),("itemId"=String,Path),("baseRevision"=i64,Query)),security(("bearer"=[])),responses((status=200,body=Revision),(status=409,body=Current)),tag="sharing")]
async fn legacy_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, kind, id)): Path<(String, String, String)>,
    params: std::result::Result<Query<RevisionQuery>, QueryRejection>,
) -> Result<Response> {
    let token = token(&headers)?;
    let params = query(params)?;
    state
        .blocking(move |db| {
            db.delete_content(&token, &campaign, &kind, &id, params.base_revision, true)
        })
        .await
        .map(written)
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}/changes",params(("campaignId"=String,Path),("since"=Option<String>,Query)),security(("bearer"=[])),responses((status=200,body=Changes),(status=403,body=ErrorBody)),tag="sharing")]
async fn changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
    params: std::result::Result<Query<Since>, QueryRejection>,
) -> Result<Json<Changes>> {
    let token = token(&headers)?;
    let params = query(params)?;
    state
        .blocking(move |db| db.changes(&token, &campaign, &params.since))
        .await
        .map(Json)
}
#[utoipa::path(put,path="/api/v1/campaigns/{campaignId}/content/{kind}/{itemId}",params(("campaignId"=String,Path),("kind"=String,Path),("itemId"=String,Path)),request_body=WriteContent,security(("bearer"=[])),responses((status=200,body=Revision),(status=409,body=Current),(status=404,body=ErrorBody)),tag="content")]
async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, kind, id)): Path<(String, String, String)>,
    input: std::result::Result<Json<WriteContent>, JsonRejection>,
) -> Result<Response> {
    let token = token(&headers)?;
    let input = body(input)?;
    state
        .blocking(move |db| db.write_content(&token, &campaign, &kind, &id, &input, false))
        .await
        .map(written)
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}/content/{kind}/{itemId}",params(("campaignId"=String,Path),("kind"=String,Path),("itemId"=String,Path)),security(("bearer"=[])),responses((status=200,body=ContentItem),(status=404,body=ErrorBody)),tag="content")]
async fn detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, kind, id)): Path<(String, String, String)>,
) -> Result<Json<ContentItem>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.content_item(&token, &campaign, &kind, &id))
        .await
        .map(Json)
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}/content",params(("campaignId"=String,Path),("after"=Option<String>,Query)),security(("bearer"=[])),responses((status=200,body=ContentList)),tag="content")]
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
    params: std::result::Result<Query<After>, QueryRejection>,
) -> Result<Json<ContentList>> {
    let token = token(&headers)?;
    let params = query(params)?;
    state
        .blocking(move |db| db.content_list(&token, &campaign, &params.after))
        .await
        .map(Json)
}
#[utoipa::path(delete,path="/api/v1/campaigns/{campaignId}/content/{kind}/{itemId}",params(("campaignId"=String,Path),("kind"=String,Path),("itemId"=String,Path),("baseRevision"=i64,Query)),security(("bearer"=[])),responses((status=200,body=Revision),(status=409,body=Current)),tag="content")]
async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, kind, id)): Path<(String, String, String)>,
    params: std::result::Result<Query<RevisionQuery>, QueryRejection>,
) -> Result<Response> {
    let token = token(&headers)?;
    let params = query(params)?;
    state
        .blocking(move |db| {
            db.delete_content(&token, &campaign, &kind, &id, params.base_revision, false)
        })
        .await
        .map(written)
}
#[utoipa::path(delete,path="/api/v1/campaigns/{campaignId}/content/{kind}/{itemId}/sharing",params(("campaignId"=String,Path),("kind"=String,Path),("itemId"=String,Path),("baseRevision"=i64,Query)),security(("bearer"=[])),responses((status=200,body=Revision),(status=409,body=Current)),tag="content")]
async fn unshare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, kind, id)): Path<(String, String, String)>,
    params: std::result::Result<Query<RevisionQuery>, QueryRejection>,
) -> Result<Response> {
    let token = token(&headers)?;
    let params = query(params)?;
    state
        .blocking(move |db| db.unshare_content(&token, &campaign, &kind, &id, params.base_revision))
        .await
        .map(written)
}
#[utoipa::path(post,path="/api/v1/campaigns/{campaignId}/attachments",params(("campaignId"=String,Path),("X-Content-SHA256"=String,Header)),request_body(content((BinaryImage="image/png"),(BinaryImage="image/jpeg"),(BinaryImage="image/webp"))),security(("bearer"=[])),responses((status=201,body=Attachment),(status=400,body=ErrorBody),(status=413,body=ErrorBody)),tag="attachments")]
async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(campaign): Path<String>,
    bytes: std::result::Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Attachment>)> {
    let token = token(&headers)?;
    let mime = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::invalid)?
        .to_string();
    let checksum = headers
        .get("x-content-sha256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::invalid)?
        .to_string();
    let bytes = bytes.map_err(|error| {
        ApiError(
            if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            },
            "Invalid or oversized image body.",
        )
    })?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .blocking(move |db| {
                    db.upload_attachment(&token, &campaign, &bytes, &mime, &checksum)
                })
                .await?,
        ),
    ))
}
#[utoipa::path(get,path="/api/v1/campaigns/{campaignId}/attachments/{attachmentId}",params(("campaignId"=String,Path),("attachmentId"=String,Path)),security(("bearer"=[])),responses((status=200,content((BinaryImage="image/png"),(BinaryImage="image/jpeg"),(BinaryImage="image/webp"))),(status=404,body=ErrorBody)),tag="attachments")]
async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, id)): Path<(String, String)>,
) -> Result<Response> {
    let token = token(&headers)?;
    let result = state
        .blocking(move |db| db.download_attachment(&token, &campaign, &id))
        .await?;
    let mut reply = result.bytes.into_response();
    let h = reply.headers_mut();
    h.insert(
        "content-type",
        result
            .metadata
            .mime
            .parse()
            .map_err(|_| ApiError::internal())?,
    );
    h.insert(
        "x-content-sha256",
        result
            .metadata
            .sha256
            .parse()
            .map_err(|_| ApiError::internal())?,
    );
    h.insert(
        "content-disposition",
        format!(
            "attachment; filename=\"{}.{}\"",
            result.metadata.id,
            match result.metadata.mime.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                _ => "webp",
            }
        )
        .parse()
        .map_err(|_| ApiError::internal())?,
    );
    Ok(reply)
}
#[utoipa::path(delete,path="/api/v1/campaigns/{campaignId}/attachments/{attachmentId}",params(("campaignId"=String,Path),("attachmentId"=String,Path)),security(("bearer"=[])),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="attachments")]
async fn remove_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((campaign, id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.delete_attachment(&token, &campaign, &id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(OpenApi)]
#[openapi(
    paths(
        legacy_put,
        legacy_delete,
        changes,
        save,
        detail,
        list,
        remove,
        unshare,
        upload,
        download,
        remove_attachment
    ),
    components(schemas(
        WriteItem,
        WriteContent,
        Visibility,
        Revision,
        Current,
        Item,
        ContentItem,
        Changes,
        ContentList,
        Attachment,
        BinaryImage
    ))
)]
pub(crate) struct ContentContract;
