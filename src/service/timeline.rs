//! Bounded, source-aware interpretations. This is not a complete life history.
use super::ApiError;
use crate::runtime::Runtime;
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineQuery {
    include_history: Option<bool>,
    limit: Option<u32>,
}

pub async fn timeline(
    State(rt): State<Runtime>,
    query: Result<Query<TimelineQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let q = query.map_err(|_| ApiError::invalid("自传查询参数无效"))?.0;
    let mut view = rt
        .store
        .autobiography(q.include_history.unwrap_or(false), q.limit.unwrap_or(30))
        .await?;
    view["view"] = Value::String("autobiographical_interpretations".into());
    Ok(Json(view))
}
