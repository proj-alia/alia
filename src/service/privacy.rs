//! Authenticated owner erasure: acceptance is distinct from physical completion.
use super::{
    ApiError, PrivacySafeResponse,
    api::{key, version},
    json_response,
};
use crate::runtime::Runtime;
use axum::{
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use uuid::Uuid;

fn no_query(query: Option<String>) -> Result<(), ApiError> {
    if query.is_some_and(|q| !q.is_empty()) {
        return Err(ApiError::invalid("此接口不接受查询字段"));
    }
    Ok(())
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "feature_unavailable",
        "删除日志未就绪，删除入口不可用",
    )
}

pub async fn delete_memory(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    no_query(query)?;
    if !body.is_empty() {
        return Err(ApiError::invalid("删除接口不接受请求正文"));
    }
    let Path(id) = path.map_err(|_| ApiError::invalid("记忆标识无效"))?;
    let key = key(&headers)?;
    let version = version(&headers)?;
    let journal = rt.deletion_journal().ok_or_else(unavailable)?;
    let receipt = rt
        .store
        .begin_memory_deletion(key, id, version, journal)
        .await?;
    rt.wake_privacy();
    rt.wake.notify_one();
    let mut response = json_response(StatusCode::ACCEPTED, &receipt, None);
    // Only this durably committed opaque receipt may cross its own epoch change.
    response.extensions_mut().insert(PrivacySafeResponse);
    Ok(response)
}

pub async fn progress(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    no_query(query)?;
    let Path(id) = path.map_err(|_| ApiError::invalid("删除操作标识无效"))?;
    rt.deletion_journal().ok_or_else(unavailable)?;
    let progress = rt.store.privacy_operation(id).await?;
    Ok(json_response(StatusCode::OK, &progress, None))
}
