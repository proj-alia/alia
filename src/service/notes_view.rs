//! Explicit owner browsing; never enters the model/tool execution path.
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
pub struct NotesQuery {
    q: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteQuery {
    offset: Option<u32>,
    max_chars: Option<u32>,
}

pub async fn notes(
    State(rt): State<Runtime>,
    query: Result<Query<NotesQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let q = query.map_err(|_| ApiError::invalid("笔记查询参数无效"))?.0;
    Ok(Json(
        rt.store
            .public_notes(
                q.q.as_deref().unwrap_or(""),
                q.cursor.as_deref(),
                q.limit.unwrap_or(30),
            )
            .await?,
    ))
}

pub async fn note(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    query: Result<Query<NoteQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("笔记标识无效"))?;
    let q = query.map_err(|_| ApiError::invalid("笔记片段参数无效"))?.0;
    Ok(Json(
        rt.store
            .public_note(id, q.offset.unwrap_or(0), q.max_chars.unwrap_or(2000))
            .await?,
    ))
}
