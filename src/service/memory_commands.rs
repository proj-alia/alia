//! Owner memory reads/corrections share the existing HTTP and transaction gates.
use super::{
    ApiError,
    api::{Strict, key, version},
    json_response,
};
use crate::{adapters::memory_commands::MemoryRevision, runtime::Runtime};
use axum::{
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

pub async fn history(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    query: Result<Query<HistoryQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("记忆标识无效"))?;
    let q = query.map_err(|_| ApiError::invalid("记忆历史参数无效"))?.0;
    Ok(axum::Json(
        rt.store
            .owner_memory_history(id, q.cursor.as_deref(), q.limit.unwrap_or(30))
            .await?,
    ))
}

pub async fn memory(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    if query.as_deref().is_some_and(|query| !query.is_empty()) {
        return Err(ApiError::invalid("记忆详情不接受查询字段"));
    }
    let Path(id) = path.map_err(|_| ApiError::invalid("记忆标识无效"))?;
    let view = rt.store.owner_memory(id).await?;
    let memory_version = view["memory"]["version"].as_i64();
    Ok(json_response(StatusCode::OK, &view, memory_version))
}

pub async fn revise(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    headers: HeaderMap,
    Strict(revision): Strict<MemoryRevision>,
) -> Result<Response, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("记忆标识无效"))?;
    let receipt = rt
        .store
        .revise_owner_memory(key(&headers)?, id, version(&headers)?, &revision)
        .await?;
    rt.wake.notify_one();
    Ok(json_response(
        StatusCode::CREATED,
        &receipt,
        Some(receipt.memory_version),
    ))
}
