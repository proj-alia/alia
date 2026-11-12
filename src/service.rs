use crate::{adapters, runtime::Runtime};
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Serialize;
use serde_json::json;
mod api;
mod beliefs_view;
mod developments;
mod documents_view;
mod events;
mod memory_commands;
mod notes_view;
mod overview;
mod privacy;
mod relationships_view;
mod source_events;
mod timeline;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

pub fn router(runtime: Runtime, web_dir: PathBuf) -> Router {
    let api = Router::new()
        .route("/initialize", post(api::initialize))
        .route("/state", get(api::state))
        .route("/overview", get(overview::overview))
        .route("/beliefs", get(beliefs_view::beliefs))
        .route("/beliefs/{id}", get(beliefs_view::belief))
        .route("/messages", get(api::messages).post(api::message))
        .route("/operations/{id}", get(api::operation))
        .route(
            "/documents",
            get(documents_view::documents)
                .post(api::document)
                .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/documents/{id}", get(documents_view::document))
        .route("/notes", get(notes_view::notes))
        .route("/notes/{id}", get(notes_view::note))
        .route("/relationships", get(relationships_view::relationships))
        .route("/relationships/{id}", get(relationships_view::relationship))
        .route(
            "/relationships/{id}/events",
            get(relationships_view::relationship_events),
        )
        .route("/source-events/{id}", get(source_events::source_event))
        .route("/memories", get(api::memories))
        .route("/memories/retrieve", get(api::retrieve_memories))
        .route("/runs", get(api::runs))
        .route("/runs/{id}", get(api::trace))
        .route("/audit", get(api::audit))
        .route("/diagnostics", get(api::diagnostics))
        .route("/management", get(api::management))
        .route("/persona", get(api::persona_view))
        .route("/sandbox/validate", post(api::sandbox))
        .route("/goals", get(api::goals).post(api::create_goal))
        .route("/autonomy/pause", post(api::pause))
        .route("/autonomy/resume", post(api::resume))
        .route("/lifecycle/transitions", post(api::lifecycle))
        .route("/settings", put(api::settings))
        .route("/events", get(events::subscribe))
        .route("/goals/{id}/transitions", post(api::change_goal))
        .route(
            "/memories/{id}",
            get(memory_commands::memory).delete(privacy::delete_memory),
        )
        .route("/memories/{id}/revisions", post(memory_commands::revise))
        .route("/memories/{id}/history", get(memory_commands::history))
        .route("/privacy-operations/{id}", get(privacy::progress))
        .route("/developments", get(developments::list))
        .route("/developments/{id}", get(developments::detail))
        .route("/timeline", get(timeline::timeline))
        .route("/developments/{id}/revert", post(developments::revert))
        .route("/exports", post(unavailable))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(axum::extract::DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn_with_state(
            runtime.clone(),
            authenticate,
        ));
    Router::new()
        .route("/healthz", get(health))
        .nest("/api/v1", api)
        .fallback_service(
            ServeDir::new(&web_dir).not_found_service(ServeFile::new(web_dir.join("index.html"))),
        )
        .with_state(runtime)
}

async fn health(State(runtime): State<Runtime>) -> Response {
    let ready = runtime.store.privacy_ready() && adapters::database_ready(&runtime.database).await;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({"status": if ready {"ready"} else {"not_ready"}})),
    )
        .into_response()
}

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    retryable: bool,
}
/// Only committed opaque deletion receipts may cross a generation change.
/// This is a server-side extension, never a client header or a model decision.
#[derive(Clone, Copy)]
pub(crate) struct PrivacySafeResponse;
impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            retryable: false,
        }
    }
    fn invalid(message: &'static str) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request", message)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status,Json(json!({"error":{"code":self.code,"message":self.message,"retryable":self.retryable,"request_id":Uuid::new_v4()}}))).into_response()
    }
}
impl From<crate::adapters::store::StoreError> for ApiError {
    fn from(error: crate::adapters::store::StoreError) -> Self {
        use crate::adapters::store::StoreError as E;
        match error {
            E::PrivacyUnavailable => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "privacy_unavailable",
                message: "隐私日志尚未恢复，访问暂时关闭",
                retryable: true,
            },
            E::NotInitialized => {
                Self::new(StatusCode::CONFLICT, "not_initialized", "个体尚未初始化")
            }
            E::NotFound => Self::new(StatusCode::NOT_FOUND, "not_found", "资源不存在或已不可用"),
            E::AlreadyInitialized => Self::new(
                StatusCode::CONFLICT,
                "already_initialized",
                "当前安装已有个体",
            ),
            E::IdempotencyConflict => Self::new(
                StatusCode::CONFLICT,
                "idempotency_conflict",
                "此幂等键已用于不同请求",
            ),
            E::StaleState | E::LeaseLost => Self::new(
                StatusCode::CONFLICT,
                "version_conflict",
                "状态已经变化，请重新读取",
            ),
            E::Inactive => Self::new(StatusCode::CONFLICT, "inactive", "当前生命周期不允许此操作"),
            E::BudgetUnavailable | E::CallLimit => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "budget_unavailable",
                "本次执行额度不可用",
            ),
            E::Invalid(_) | E::Proposal(_) => Self::invalid("请求不符合当前状态或配置规则"),
            E::RetrievalUnavailable => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "retrieval_unavailable",
                message: "本地记忆编码暂不可用；没有静默切换检索方式",
                retryable: true,
            },
            E::PersonAmbiguous => Self::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "person_ambiguous",
                "该姓名或别名对应多个人物，请明确身份后查询",
            ),
            E::Database(_) | E::Decode(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "storage_unavailable",
                message: "暂时无法完成存储操作",
                retryable: true,
            },
        }
    }
}
fn json_response(status: StatusCode, value: &impl Serialize, version: Option<i64>) -> Response {
    let mut response = (status, Json(value)).into_response();
    if let Some(version) = version {
        response.headers_mut().insert(
            "etag",
            format!("\"{version}\"").parse().expect("numeric ETag"),
        );
    }
    response
}
fn error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    ApiError::new(status, code, message).into_response()
}
async fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "not_found", "接口不存在")
}
async fn method_not_allowed() -> Response {
    error(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "此接口不支持该方法",
    )
}

async fn authenticate(State(runtime): State<Runtime>, request: Request, next: Next) -> Response {
    let privacy_generation = runtime.store.privacy_generation();
    let authorized = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|token| runtime.authorized(token));
    let access_valid = authorized
        && request.headers().get_all("authorization").iter().count() == 1
        && same_origin(request.headers());
    let mut response =
        if !authorized || request.headers().get_all("authorization").iter().count() != 1 {
            error(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "需要有效的实例访问令牌",
            )
        } else if !same_origin(request.headers()) {
            error(
                StatusCode::FORBIDDEN,
                "cross_origin_denied",
                "不接受跨源访问",
            )
        } else if !runtime.store.privacy_ready() {
            ApiError::from(crate::adapters::store::StoreError::PrivacyUnavailable).into_response()
        } else {
            next.run(request).await
        };
    // A handler that started before an uncertain journal/DB commit cannot send
    // a stale private snapshot after the shared fence has been raised.
    if access_valid
        && (!runtime.store.privacy_ready()
            || (runtime.store.privacy_generation() != privacy_generation
                && response.extensions().get::<PrivacySafeResponse>().is_none()))
    {
        response =
            ApiError::from(crate::adapters::store::StoreError::PrivacyUnavailable).into_response();
    }
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    response
}

fn same_origin(headers: &axum::http::HeaderMap) -> bool {
    if headers
        .get("sec-fetch-site")
        .is_some_and(|h| h == "cross-site")
    {
        return false;
    }
    if headers.get_all("origin").iter().count() > 1 {
        return false;
    }
    let Some(origin) = headers.get("origin") else {
        return true;
    };
    let Some(origin) = origin
        .to_str()
        .ok()
        .and_then(|v| reqwest::Url::parse(v).ok())
    else {
        return false;
    };
    if !matches!(origin.scheme(), "http" | "https")
        || origin.username() != ""
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return false;
    }
    let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Ok(target) = reqwest::Url::parse(&format!("{}://{host}/", origin.scheme())) else {
        return false;
    };
    origin.origin() == target.origin()
}

async fn unavailable() -> Response {
    error(
        StatusCode::NOT_IMPLEMENTED,
        "feature_unavailable",
        "个体功能尚未实现",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    const TOKEN: &str = "development-test-token-not-a-real-secret";

    #[tokio::test]
    async fn privacy_fence_is_shared_and_closes_http_and_model_storage_before_database_access() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://127.0.0.1:9/unused")
            .unwrap();
        let rt = Runtime::new(pool, TOKEN).unwrap();
        let cloned = rt.store.clone();
        rt.store.fence_privacy();
        assert!(!cloned.privacy_ready());
        assert!(matches!(
            cloned.snapshot().await,
            Err(crate::adapters::store::StoreError::PrivacyUnavailable)
        ));
        assert!(matches!(
            cloned.model_context(Uuid::new_v4(), vec![]).await,
            Err(crate::adapters::store::StoreError::PrivacyUnavailable)
        ));
        let app = router(rt, PathBuf::from("web/dist"));
        for path in ["/api/v1/state", "/api/v1/timeline", "/api/v1/developments"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("authorization", format!("Bearer {TOKEN}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(response.headers()["cache-control"], "no-store");
        }
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/state")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        cloned.set_privacy_ready(true);
        assert!(cloned.privacy_ready());
    }

    #[tokio::test]
    async fn a_handler_cannot_send_a_private_response_after_an_uncertain_privacy_commit() {
        async fn private(State(rt): State<Runtime>) -> Json<serde_json::Value> {
            rt.store.fence_privacy();
            // Even a successful recovery cannot make the old snapshot current.
            rt.store.set_privacy_ready(true);
            Json(json!({"private":"SYNTHETIC-LATE-PRIVATE"}))
        }
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://127.0.0.1:9/unused")
            .unwrap();
        let rt = Runtime::new(pool, TOKEN).unwrap();
        let app = Router::new()
            .route("/private", get(private))
            .layer(middleware::from_fn_with_state(rt.clone(), authenticate))
            .with_state(rt);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/private")
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!String::from_utf8_lossy(&bytes).contains("SYNTHETIC-LATE-PRIVATE"));
    }

    fn app() -> Router {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        router(
            Runtime::new(pool, TOKEN).unwrap(),
            PathBuf::from("web/dist"),
        )
    }

    #[tokio::test]
    async fn api_requires_token_even_for_unimplemented_routes() {
        for token in [None, Some("Bearer wrong")] {
            let mut builder = Request::builder().uri("/api/v1/state");
            if let Some(token) = token {
                builder = builder.header("authorization", token);
            }
            let response = app()
                .oneshot(builder.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn unimplemented_api_does_not_return_html_or_fake_success() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/exports")
                    .method("POST")
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "feature_unavailable");
    }
}
