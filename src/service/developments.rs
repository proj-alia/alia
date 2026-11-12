//! Development reads and owner reverts use the existing provenance-aware Store.
use super::{
    ApiError,
    api::{Strict, key, version},
    json_response,
};
use crate::runtime::Runtime;
use axum::{
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    include_history: Option<bool>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevertRequest {
    reason: String,
}

pub async fn list(
    State(rt): State<Runtime>,
    query: Result<Query<ListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let query = query.map_err(|_| ApiError::invalid("发展列表参数无效"))?.0;
    Ok(axum::Json(
        rt.store
            .developments(
                query.include_history.unwrap_or(false),
                query.limit.unwrap_or(30),
            )
            .await?,
    ))
}

pub async fn detail(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    no_query(query)?;
    let Path(id) = path.map_err(|_| ApiError::invalid("发展标识无效"))?;
    let view = rt.store.development(id).await?;
    let version = view["development"]["version"].as_i64();
    Ok(json_response(StatusCode::OK, &view, version))
}

pub async fn revert(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    Strict(request): Strict<RevertRequest>,
) -> Result<Response, ApiError> {
    no_query(query)?;
    let Path(id) = path.map_err(|_| ApiError::invalid("发展标识无效"))?;
    if request.reason.trim().is_empty()
        || request.reason.contains('\0')
        || request.reason.chars().count() > 1000
    {
        return Err(ApiError::invalid(
            "撤回理由须为1至1000个字符，不能仅含空白或包含空字符",
        ));
    }
    // If-Match is this development's version. Store replays a committed intent
    // before checking the lifecycle/global CAS of a new command.
    let receipt = rt
        .store
        .owner_revert_development(key(&headers)?, id, version(&headers)?, &request.reason)
        .await?;
    rt.wake.notify_one();
    Ok(json_response(
        StatusCode::OK,
        &receipt,
        Some(receipt.development_version),
    ))
}

fn no_query(query: Option<String>) -> Result<(), ApiError> {
    if query.as_deref().is_some_and(|query| !query.is_empty()) {
        return Err(ApiError::invalid("发展详情和撤回不接受查询字段"));
    }
    Ok(())
}
