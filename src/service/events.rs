use super::ApiError;
use crate::{
    adapters::notifications::{Notification, NotificationError},
    runtime::Runtime,
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use serde::Deserialize;
use serde_json::json;
use std::{collections::VecDeque, convert::Infallible, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    after: Option<String>,
}
fn parse(s: &str) -> Result<i64, ApiError> {
    if s.is_empty() || s.len() > 19 || !s.bytes().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::invalid("事件游标必须为非负整数"));
    }
    s.parse().map_err(|_| ApiError::invalid("事件游标超出范围"))
}
impl From<NotificationError> for ApiError {
    fn from(e: NotificationError) -> Self {
        match e {
            NotificationError::Storage(e) => e.into(),
            NotificationError::Expired => Self::new(
                StatusCode::GONE,
                "event_cursor_expired",
                "通知已过期，请重新读取 /state 快照并用 event_cursor 订阅",
            ),
            NotificationError::Future => Self::invalid("事件游标超过已提交历史"),
        }
    }
}
struct Subscription {
    rt: Runtime,
    shutdown: tokio::sync::watch::Receiver<bool>,
    _permit: tokio::sync::OwnedSemaphorePermit,
    cursor: i64,
    queue: VecDeque<Notification>,
    greeting: bool,
    privacy_generation: u64,
}
pub async fn subscribe(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    query: Result<Query<Cursor>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, ApiError> {
    let privacy_generation = rt.store.privacy_generation();
    let query = query.map_err(|_| ApiError::invalid("事件查询参数无效"))?.0;
    let values: Vec<_> = headers.get_all("last-event-id").iter().collect();
    if values.len() > 1 {
        return Err(ApiError::invalid("Last-Event-ID 不允许重复"));
    }
    let header = values
        .first()
        .map(|h| {
            h.to_str()
                .map_err(|_| ApiError::invalid("事件游标无效"))
                .and_then(parse)
        })
        .transpose()?;
    let after = query.after.as_deref().map(parse).transpose()?;
    if header.is_some() && after.is_some() && header != after {
        return Err(ApiError::invalid("事件游标互相冲突"));
    }
    let shutdown = rt.shutdown.subscribe();
    if *shutdown.borrow() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "shutting_down",
            "服务正在停止",
        ));
    }
    let permit = rt.event_slots.clone().try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "event_connections_exhausted",
            "实时连接已达上限，请稍后重试",
        )
    })?;
    rt.store.prune_notifications().await?;
    let batch = rt.store.notifications(header.or(after)).await?;
    let stream = futures_util::stream::unfold(
        Subscription {
            rt,
            shutdown,
            _permit: permit,
            cursor: batch.cursor,
            queue: batch.items.into(),
            greeting: true,
            privacy_generation,
        },
        |mut s| async move {
            loop {
                if *s.shutdown.borrow()
                    || !s.rt.store.privacy_ready()
                    || s.rt.store.privacy_generation() != s.privacy_generation
                {
                    return None;
                }
                // Also flush headers immediately for an idle subscription.
                if s.greeting {
                    s.greeting = false;
                    return Some((
                        Ok::<_, Infallible>(
                            Event::default()
                                .comment("connected")
                                .id(s.cursor.to_string()),
                        ),
                        s,
                    ));
                }
                if let Some(n) = s.queue.pop_front() {
                    s.cursor = n.sequence;
                    let event = Event::default().id(n.sequence.to_string());
                    let event = match n.resource {
                        Some((kind, id, version)) => event
                            .event(kind)
                            .data(json!({"resource_id":id,"version":version}).to_string()),
                        None => event.comment("cursor"),
                    };
                    return Some((Ok(event), s));
                }
                tokio::select! { biased;
                    _=s.shutdown.changed()=>return None,
                    _=tokio::time::sleep(Duration::from_secs(1))=>{},
                }
                if !s.rt.store.privacy_ready()
                    || s.rt.store.privacy_generation() != s.privacy_generation
                {
                    return None;
                }
                let result = tokio::select! { biased;
                    _=s.shutdown.changed()=>return None,
                    result=s.rt.store.notifications(Some(s.cursor))=>result,
                };
                match result {
                    Ok(batch) => s.queue = batch.items.into(),
                    // HTTP status cannot change after headers. Close; the next connection
                    // receives a typed 410/404/503 response and performs snapshot recovery.
                    Err(_) => return None,
                }
            }
        },
    );
    let mut response = Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response();
    response
        .headers_mut()
        .insert("x-accel-buffering", "no".parse().unwrap());
    Ok(response)
}
