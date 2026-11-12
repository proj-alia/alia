use super::{ApiError, json_response};
use crate::{
    adapters::api::{Listing, OwnerChange},
    domain::{
        LifecycleCommand,
        config::{PersonaSeed, RuntimeRules, parse_strict_json},
    },
    runtime::{Runtime, capability::IMPLEMENTED_CAPABILITIES},
};
use axum::{
    Json,
    body::Bytes,
    extract::{FromRequest, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use uuid::Uuid;

pub struct Strict<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Strict<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        if !req
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|s| s.trim().eq_ignore_ascii_case("application/json"))
            })
        {
            return Err(ApiError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "请求必须使用 application/json",
            ));
        }
        let bytes = Bytes::from_request(req, state).await.map_err(|_| {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "请求正文超过限制或无法读取",
            )
        })?;
        let value = parse_strict_json(&bytes, 2 * 1024 * 1024)
            .map_err(|_| ApiError::invalid("JSON 无效或包含重复字段"))?;
        serde_json::from_value(value)
            .map(Strict)
            .map_err(|_| ApiError::invalid("请求字段不符合接口定义"))
    }
}
fn single<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, ApiError> {
    if headers.get_all(name).iter().count() != 1 {
        return Err(ApiError::invalid("缺少必要请求头或请求头重复"));
    }
    headers
        .get(name)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| ApiError::invalid("请求头无效"))
}
pub(super) fn key(h: &HeaderMap) -> Result<&str, ApiError> {
    let k = single(h, "idempotency-key")?;
    if k.is_empty() || k.len() > 128 || !k.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(ApiError::invalid(
            "Idempotency-Key 必须为 1 至 128 个可见 ASCII 字符",
        ));
    }
    Ok(k)
}
pub(super) fn version(h: &HeaderMap) -> Result<i64, ApiError> {
    let v = single(h, "if-match")?;
    let v = v
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| ApiError::invalid("If-Match 必须是带引号的版本号"))?;
    if !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::invalid("If-Match 无效"));
    }
    v.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::invalid("If-Match 无效"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Initialize {
    persona: Value,
    runtime: Value,
}
pub async fn initialize(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(input): Strict<Initialize>,
) -> Result<axum::response::Response, ApiError> {
    let seed =
        PersonaSeed::validate(input.persona).map_err(|_| ApiError::invalid("人格种子配置无效"))?;
    let config = RuntimeRules::validate(input.runtime, IMPLEMENTED_CAPABILITIES)
        .map_err(|_| ApiError::invalid("运行配置无效或包含未实现能力"))?;
    let receipt = rt.store.initialize(key(&headers)?, &seed, &config).await?;
    rt.wake.notify_one();
    Ok(json_response(
        StatusCode::CREATED,
        &receipt,
        Some(receipt.version),
    ))
}
pub async fn state(State(rt): State<Runtime>) -> Result<axum::response::Response, ApiError> {
    let mut value = rt.store.public_state().await?;
    value["model"] = json!(rt.model_status);
    value["indexing"] = json!({"enabled":rt.indexing_enabled,"profile":if rt.indexing_enabled {Some(crate::domain::retrieval::INDEX_PROFILE)} else {None},"retrieval_mode":if rt.store.semantic_enabled(){"hybrid"}else{"lexical"}});
    let version = value["version"].as_i64();
    Ok(json_response(StatusCode::OK, &value, version))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    text: String,
    conversation_id: Option<Uuid>,
}
pub async fn message(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(input): Strict<Message>,
) -> Result<axum::response::Response, ApiError> {
    if input.text.contains('\0') {
        return Err(ApiError::invalid("消息不能包含空字符"));
    }
    let receipt = rt
        .store
        .submit_message(key(&headers)?, &input.text, input.conversation_id)
        .await?;
    rt.wake.notify_one();
    Ok(json_response(
        StatusCode::ACCEPTED,
        &json!({"operation_id":receipt.operation_id,"event_id":receipt.event_id,"status":"accepted","version":receipt.version}),
        None,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    title: String,
    text: String,
    source_label: String,
}
pub async fn document(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(input): Strict<Document>,
) -> Result<axum::response::Response, ApiError> {
    let receipt = rt
        .store
        .import_document(
            key(&headers)?,
            &input.title,
            &input.text,
            &input.source_label,
        )
        .await?;
    rt.wake.notify_one();
    Ok(json_response(StatusCode::CREATED, &receipt, None))
}
pub async fn operation(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
) -> Result<Json<Value>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("运行标识无效"))?;
    Ok(Json(rt.store.public_operation(id).await?))
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    order: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MemoryQuery {
    cursor: Option<String>,
    limit: Option<u32>,
    q: Option<String>,
    validity: Option<String>,
}
fn page(query: PageQuery, kind: &str) -> Result<(i64, Uuid, u32), ApiError> {
    let limit = query.limit.unwrap_or(30);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit 必须在 1 至 100 之间"));
    }
    let mut sequence = 0;
    let mut id = Uuid::nil();
    if let Some(cursor) = query.cursor {
        let rest = cursor
            .strip_prefix(&format!("v1:{kind}:"))
            .ok_or_else(|| ApiError::invalid("分页游标无效"))?;
        if kind.starts_with("messages") || kind == "audit" {
            sequence = rest
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| ApiError::invalid("分页游标无效"))?;
        } else {
            id = Uuid::parse_str(rest).map_err(|_| ApiError::invalid("分页游标无效"))?;
        }
    }
    Ok((sequence, id, limit))
}
async fn listing(
    rt: Runtime,
    query: PageQuery,
    kind: Listing,
    name: &str,
    q: &str,
    validity: Option<&str>,
) -> Result<Json<Value>, ApiError> {
    if !name.starts_with("messages") && query.order.is_some() {
        return Err(ApiError::invalid("此列表不支持排序参数"));
    }
    let (sequence, id, limit) = page(query, name)?;
    let mut items = rt
        .store
        .public_list(kind, sequence, id, limit + 1, q, validity)
        .await?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next = if more {
        items.last().map(|item| {
            format!(
                "v1:{name}:{}",
                if name.starts_with("messages") || name == "audit" {
                    item["sequence"].to_string()
                } else {
                    item["id"].as_str().unwrap_or_default().to_owned()
                }
            )
        })
    } else {
        None
    };
    Ok(Json(json!({"items":items,"next_cursor":next})))
}
pub async fn messages(
    State(rt): State<Runtime>,
    query: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let query = query.map_err(|_| ApiError::invalid("查询参数无效"))?.0;
    let descending = match query.order.as_deref() {
        None | Some("asc") => false,
        Some("desc") => true,
        _ => return Err(ApiError::invalid("排序参数无效")),
    };
    listing(
        rt,
        query,
        if descending {
            Listing::RecentMessages
        } else {
            Listing::Messages
        },
        if descending {
            "messages_desc"
        } else {
            "messages"
        },
        "",
        None,
    )
    .await
}
pub async fn goals(
    State(rt): State<Runtime>,
    query: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    listing(
        rt,
        query.map_err(|_| ApiError::invalid("查询参数无效"))?.0,
        Listing::Goals,
        "goals",
        "",
        None,
    )
    .await
}
pub async fn memories(
    State(rt): State<Runtime>,
    query: Result<Query<MemoryQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let query = query.map_err(|_| ApiError::invalid("查询参数无效"))?.0;
    let q = query.q.unwrap_or_default();
    if q.chars().count() > 1000 || q.contains('\0') {
        return Err(ApiError::invalid("检索文本无效"));
    }
    let validity = query.validity.as_deref();
    if validity.is_some_and(|v| !matches!(v, "active" | "superseded" | "disputed" | "stale")) {
        return Err(ApiError::invalid("记忆状态无效"));
    }
    listing(
        rt,
        PageQuery {
            order: None,
            cursor: query.cursor,
            limit: query.limit,
        },
        Listing::Memories,
        "memories",
        &q,
        validity,
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalQuery {
    q: String,
    scope: Option<String>,
    person_id: Option<Uuid>,
    person_name: Option<String>,
    at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    include_history: bool,
    limit: Option<u32>,
}
pub async fn retrieve_memories(
    State(rt): State<Runtime>,
    query: Result<Query<RetrievalQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let q = query.map_err(|_| ApiError::invalid("检索参数无效"))?.0;
    let request = crate::domain::retrieval::RetrievalRequest {
        text: q.q,
        scope: q.scope,
        subject_id: None,
        person_id: q.person_id,
        person_name: q.person_name,
        at: q.at.unwrap_or_else(chrono::Utc::now),
        include_history: q.include_history,
        limit: q.limit.unwrap_or(10),
    };
    Ok(Json(rt.store.public_retrieval(&request).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    action: LifecycleCommand,
}
async fn change(
    rt: Runtime,
    headers: HeaderMap,
    change: OwnerChange,
) -> Result<axum::response::Response, ApiError> {
    let receipt = rt
        .store
        .owner_change(key(&headers)?, version(&headers)?, change)
        .await?;
    rt.wake.notify_one();
    Ok(json_response(
        StatusCode::OK,
        &receipt,
        Some(receipt.version),
    ))
}
pub async fn pause(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(_): Strict<Empty>,
) -> Result<axum::response::Response, ApiError> {
    change(rt, headers, OwnerChange::AutonomyPaused(true)).await
}
pub async fn resume(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(_): Strict<Empty>,
) -> Result<axum::response::Response, ApiError> {
    change(rt, headers, OwnerChange::AutonomyPaused(false)).await
}
pub async fn lifecycle(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(t): Strict<Transition>,
) -> Result<axum::response::Response, ApiError> {
    change(rt, headers, OwnerChange::Lifecycle(t.action)).await
}
pub async fn settings(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(rules): Strict<Value>,
) -> Result<axum::response::Response, ApiError> {
    change(rt, headers, OwnerChange::Settings(rules)).await
}

pub async fn create_goal(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    Strict(draft): Strict<crate::adapters::goal_commands::GoalDraft>,
) -> Result<axum::response::Response, ApiError> {
    let receipt = rt.store.create_owner_goal(key(&headers)?, &draft).await?;
    rt.wake.notify_one();
    Ok(json_response(
        StatusCode::CREATED,
        &receipt,
        Some(receipt.goal_version),
    ))
}
pub async fn change_goal(
    State(rt): State<Runtime>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    Strict(command): Strict<crate::adapters::goal_commands::GoalCommand>,
) -> Result<axum::response::Response, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("目标标识无效"))?;
    let receipt = rt
        .store
        .change_owner_goal(key(&headers)?, id, version(&headers)?, &command)
        .await?;
    rt.wake.notify_one();
    Ok(json_response(
        StatusCode::OK,
        &receipt,
        Some(receipt.goal_version),
    ))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RunQuery {
    cursor: Option<String>,
    limit: Option<u32>,
    status: Option<String>,
    q: Option<String>,
    mode: Option<String>,
    from: Option<String>,
    to: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AuditQuery {
    cursor: Option<String>,
    limit: Option<u32>,
    q: Option<String>,
    from: Option<String>,
    to: Option<String>,
}
fn console_time(value: Option<String>) -> Result<Option<chrono::DateTime<chrono::Utc>>, ApiError> {
    value
        .map(|s| {
            let time = chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|_| ApiError::invalid("时间必须为 RFC3339 UTC"))?;
            if time.offset().local_minus_utc() != 0 {
                return Err(ApiError::invalid("时间必须为 UTC"));
            }
            Ok(time.with_timezone(&chrono::Utc))
        })
        .transpose()
}
async fn console_listing(
    rt: Runtime,
    q: RunQuery,
    kind: Listing,
    name: &str,
) -> Result<Json<Value>, ApiError> {
    use crate::adapters::api::{ConsoleFilter, ConsolePosition};
    if q.status.as_deref().is_some_and(|s| {
        !matches!(
            s,
            "queued" | "running" | "committed" | "failed" | "cancelled" | "stale"
        )
    }) {
        return Err(ApiError::invalid("运行状态无效"));
    }
    if q.mode
        .as_deref()
        .is_some_and(|s| !matches!(s, "foreground" | "background"))
    {
        return Err(ApiError::invalid("运行模式无效"));
    }
    let text = q.q.unwrap_or_default();
    if text.chars().count() > 200 || (!text.trim().is_empty() && text.chars().any(char::is_control))
    {
        return Err(ApiError::invalid("搜索最多 200 字符且不含控制字符"));
    }
    let filter = ConsoleFilter {
        q: if text.trim().is_empty() {
            String::new()
        } else {
            text
        },
        status: q.status,
        mode: q.mode,
        from: console_time(q.from)?,
        to: console_time(q.to)?,
    };
    if matches!((filter.from,filter.to),(Some(from),Some(to)) if from>=to) {
        return Err(ApiError::invalid("from 必须早于 to"));
    }
    let limit = q.limit.unwrap_or(30);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit 必须在 1 至 100 之间"));
    }
    let filter = rt.store.normalize_console_filter(filter).await?;
    let fingerprint = filter.fingerprint()?;
    let mut position = ConsolePosition::default();
    if let Some(cursor) = q.cursor {
        let invalid = || ApiError::invalid("分页游标无效或筛选已变化");
        if cursor.starts_with("v1:") && filter.is_empty() {
            let (seq, id, _) = page(
                PageQuery {
                    cursor: Some(cursor),
                    limit: Some(limit),
                    order: None,
                },
                name,
            )?;
            position.sequence = seq;
            position.id = id;
        } else {
            let prefix = format!("v2:{name}:{fingerprint}:");
            let rest = cursor.strip_prefix(&prefix).ok_or_else(invalid)?;
            if name == "audit" {
                position.sequence = rest
                    .parse::<i64>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(invalid)?;
            } else {
                let (time, id) = rest.split_once(',').ok_or_else(invalid)?;
                position.created_at = Some(
                    chrono::DateTime::from_timestamp_micros(time.parse().map_err(|_| invalid())?)
                        .ok_or_else(invalid)?,
                );
                position.id = Uuid::parse_str(id).map_err(|_| invalid())?;
                if position.id.is_nil() {
                    return Err(invalid());
                }
            }
        }
    }
    let mut items = rt
        .store
        .console_list(kind, &filter, &position, limit + 1)
        .await?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next = if more {
        items.last().map(|item| {
            let position = if name == "audit" {
                item["sequence"].to_string()
            } else {
                let time = chrono::DateTime::parse_from_rfc3339(
                    item["created_at"].as_str().unwrap_or_default(),
                )
                .map(|t| t.timestamp_micros())
                .unwrap_or_default();
                format!("{time},{}", item["id"].as_str().unwrap_or_default())
            };
            // Preserve the pre-filter cursor shape for unchanged legacy consumers.
            if filter.is_empty() {
                format!(
                    "v1:{name}:{}",
                    if name == "audit" {
                        item["sequence"].to_string()
                    } else {
                        item["id"].as_str().unwrap_or_default().to_owned()
                    }
                )
            } else {
                format!("v2:{name}:{fingerprint}:{position}")
            }
        })
    } else {
        None
    };
    Ok(Json(json!({"items":items,"next_cursor":next})))
}
pub async fn runs(
    State(rt): State<Runtime>,
    query: Result<Query<RunQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    console_listing(
        rt,
        query.map_err(|_| ApiError::invalid("查询参数无效"))?.0,
        Listing::Runs,
        "runs",
    )
    .await
}
pub async fn trace(
    State(rt): State<Runtime>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
) -> Result<Json<Value>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::invalid("运行标识无效"))?;
    Ok(Json(rt.store.console_run(id).await?))
}
pub async fn audit(
    State(rt): State<Runtime>,
    query: Result<Query<AuditQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let q = query.map_err(|_| ApiError::invalid("查询参数无效"))?.0;
    console_listing(
        rt,
        RunQuery {
            cursor: q.cursor,
            limit: q.limit,
            q: q.q,
            from: q.from,
            to: q.to,
            ..Default::default()
        },
        Listing::Audit,
        "audit",
    )
    .await
}
pub async fn diagnostics(State(rt): State<Runtime>) -> Result<Json<Value>, ApiError> {
    let mut view = rt.store.console_diagnostics().await?;
    view["service_started_at"] = json!(rt.started_at);
    view["service_version"] = json!(env!("CARGO_PKG_VERSION"));
    Ok(Json(view))
}
pub async fn management(State(rt): State<Runtime>) -> Result<Json<Value>, ApiError> {
    let state = rt.store.public_state().await?;
    Ok(Json(
        json!({"individual_id":state["individual_id"],"model":rt.model_connection,"readiness":rt.model_status,
 "fallback":{"supported":false,"configured":false},"provider_probe_supported":false,
 "privacy":{"deletion_enabled":rt.deletion_ready(),"journal_ready":rt.deletion_ready(),"status_read_available":rt.deletion_ready()},
 "development":{"read_enabled":true,"revert_enabled":true},
 "channels":[{"id":"web","kind":"webui","credential_mode":"instance_bearer","identity_mapping":"single_individual","session_memory_isolation":false}],
 "extensions":{"plugins_supported":false,"skills_supported":false,"mcp_supported":false,"market_supported":false,"capabilities":state["capabilities"]},
 "sandbox":{"supported":["persona","runtime","decision_schema"],"model_execution":false,"tool_execution":false}}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxInput {
    kind: String,
    document_text: String,
}
pub async fn sandbox(Strict(input): Strict<SandboxInput>) -> Result<Json<Value>, ApiError> {
    if input.document_text.len() > 65536 {
        return Err(ApiError::invalid("校验文档最多64KiB"));
    }
    let checks = match input.kind.as_str() {
        "persona" => "schema_and_configuration",
        "runtime" => "schema_configuration_capabilities",
        "decision_schema" => "schema_only",
        _ => return Err(ApiError::invalid("不支持的校验类型")),
    };
    let value = parse_strict_json(input.document_text.as_bytes(), 65536);
    let valid = match (input.kind.as_str(), value) {
        ("persona", Ok(v)) => PersonaSeed::validate(v).is_ok(),
        ("runtime", Ok(v)) => RuntimeRules::validate(v, IMPLEMENTED_CAPABILITIES).is_ok(),
        ("decision_schema", Ok(_)) => {
            crate::domain::cognition::DecisionProposal::parse(input.document_text.as_bytes())
                .is_ok()
        }
        _ => false,
    };
    Ok(Json(
        json!({"kind":input.kind,"valid":valid,"checks":checks,"execution":"not_executed","state_modified":false,"observed_at":chrono::Utc::now(),"code":if valid{"validation_passed"}else{"validation_failed"}}),
    ))
}

pub async fn persona_view(State(rt): State<Runtime>) -> Result<Json<Value>, ApiError> {
    Ok(Json(rt.store.console_persona().await?))
}
