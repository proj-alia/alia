//! Owner browsing only: no mutations, models or inferred relationship objects.
use super::ApiError;
use crate::runtime::Runtime;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyQuery {}
pub async fn relationships(
    State(rt): State<Runtime>,
    query: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let q = query
        .map_err(|_| ApiError::invalid("关系目录查询参数无效"))?
        .0;
    Ok(Json(
        rt.store
            .public_relationships(q.cursor.as_deref(), q.limit.unwrap_or(30))
            .await?,
    ))
}
pub async fn relationship(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    query: Result<Query<EmptyQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("关系主体标识无效"))?;
    query.map_err(|_| ApiError::invalid("关系详情不接受查询参数"))?;
    Ok(Json(rt.store.public_relationship(id).await?))
}
pub async fn relationship_events(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    query: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("关系主体标识无效"))?;
    let q = query
        .map_err(|_| ApiError::invalid("关系时间线查询参数无效"))?
        .0;
    Ok(Json(
        rt.store
            .public_relationship_events(id, q.cursor.as_deref(), q.limit.unwrap_or(30))
            .await?,
    ))
}
