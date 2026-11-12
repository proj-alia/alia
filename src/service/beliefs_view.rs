use super::ApiError;
use crate::runtime::Runtime;
use axum::{
    Json,
    extract::{Path, Query, RawQuery, State},
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeliefsQuery {
    q: Option<String>,
    scope: Option<String>,
    status: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}
pub async fn beliefs(
    State(rt): State<Runtime>,
    query: Result<Query<BeliefsQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let q = query.map_err(|_| ApiError::invalid("信念查询参数无效"))?.0;
    Ok(Json(
        rt.store
            .public_beliefs(
                q.q.as_deref().unwrap_or(""),
                q.scope.as_deref(),
                q.status.as_deref().unwrap_or("current"),
                q.cursor.as_deref(),
                q.limit.unwrap_or(30),
            )
            .await?,
    ))
}
pub async fn belief(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Json<Value>, ApiError> {
    if query.as_deref().is_some_and(|q| !q.is_empty()) {
        return Err(ApiError::invalid("信念详情不接受查询字段"));
    }
    let Path(id) = path.map_err(|_| ApiError::invalid("信念标识无效"))?;
    Ok(Json(rt.store.public_belief(id).await?))
}
