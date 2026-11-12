//! Owner-only source metadata and explicitly allowed text fragments.
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
pub struct SourceEventQuery {
    offset: Option<u32>,
    max_chars: Option<u32>,
}

pub async fn source_event(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    query: Result<Query<SourceEventQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("来源事件标识无效"))?;
    let q = query
        .map_err(|_| ApiError::invalid("来源事件片段参数无效"))?
        .0;
    Ok(Json(
        rt.store
            .public_source_event(id, q.offset.unwrap_or(0), q.max_chars.unwrap_or(2000))
            .await?,
    ))
}
