use super::ApiError;
use crate::runtime::Runtime;
use axum::{
    Json,
    extract::{RawQuery, State},
};
use serde_json::Value;

pub async fn overview(
    State(rt): State<Runtime>,
    RawQuery(query): RawQuery,
) -> Result<Json<Value>, ApiError> {
    // This resource has no query fields; reject even an empty-valued/duplicate key.
    if query.is_some_and(|q| !q.is_empty()) {
        return Err(ApiError::invalid("概览不接受查询参数"));
    }
    Ok(Json(rt.store.public_overview().await?))
}
