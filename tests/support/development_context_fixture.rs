// Synthetic registered observations only for engineering setup;
// production trial generation and growth behavior are not certified here.
use alia::{
    adapters::{
        development::{DevelopmentCommand, DevelopmentReceipt},
        store::Store,
    },
    domain::{
        config::{PersonaSeed, RuntimeRules},
        development::*,
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "development-init",
        &PersonaSeed::parse(include_bytes!("../../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    s
}
async fn message(s: &Store) -> Uuid {
    s.submit_message(
        &Uuid::new_v4().to_string(),
        "这是实际记录的合成外部陈述；不作为真实模型成长证据",
        None,
    )
    .await
    .unwrap()
    .event_id
}
fn seed() -> Value {
    serde_json::from_slice(include_bytes!("../../config/persona.example.json")).unwrap()
}
fn traits(value: f64) -> Value {
    let mut v = seed()["personality"]["traits"].clone();
    v["curiosity"] = json!(value);
    v
}
fn draft(event: Uuid, target: DevelopmentTarget, value: Value) -> DevelopmentDraft {
    DevelopmentDraft {
        target,
        before_version: 1,
        proposed_value: value,
        proposed_description: "根据实际经历作出有界调整".into(),
        evidence_ids: vec![event],
        counterevidence_ids: vec![],
        validation_plan: "实际独立场景与反证搜索验证；保留失败，不扩大权限".into(),
        deadline: Utc::now() + Duration::days(1),
    }
}
async fn event(pool: &PgPool, kind: &str, source: &str, actor: &str, payload: Value) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'event' FROM individuals")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,$2,$3,$4,clock_timestamp(),$5,$1 FROM individuals").bind(id).bind(kind).bind(source).bind(actor).bind(payload).execute(pool).await.unwrap();
    id
}
async fn registered(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,'development_verifier' FROM individuals").bind(id).execute(pool).await.unwrap();
    let registration = event(
        pool,
        "development.verifier.registered",
        "direct_observation",
        "system",
        json!({"verifier_id":id,"kind":"measurement"}),
    )
    .await;
    sqlx::query("INSERT INTO development_verifiers(individual_id,id,kind,registration_event_id) SELECT id,$1,'measurement',$2 FROM individuals").bind(id).bind(registration).execute(pool).await.unwrap();
    id
}
fn unique(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort();
    ids.dedup();
    ids
}
async fn propose_refs(pool: &PgPool, d: &DevelopmentDraft, proposer: Uuid) -> Vec<Uuid> {
    let mut ids = d.evidence_ids.clone();
    ids.extend(&d.counterevidence_ids);
    ids.push(proposer);
    let scope = if d.target == DevelopmentTarget::TaskStrategy {
        Some(
            serde_json::from_value::<TaskStrategy>(d.proposed_value.clone())
                .unwrap()
                .scope,
        )
    } else {
        None
    };
    let (k, id) = match scope {
        Some(StrategyScope::Goal(id)) => (Some("goal"), Some(id)),
        Some(StrategyScope::Task(id)) => (Some("task"), Some(id)),
        None => (None, None),
    };
    ids.extend(id);
    let name = serde_json::to_value(d.target)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let h:Option<(Uuid,Option<Uuid>)>=sqlx::query_as("SELECT id,current_revision_id FROM development_targets WHERE target=$1 AND scope_kind IS NOT DISTINCT FROM $2 AND scope_id IS NOT DISTINCT FROM $3 AND NOT provenance_removed").bind(name).bind(k).bind(id).fetch_optional(pool).await.unwrap();
    if let Some((target, head)) = h {
        ids.push(target);
        ids.extend(head)
    }
    unique(ids)
}
async fn propose(s: &Store, pool: &PgPool, d: &DevelopmentDraft) -> DevelopmentReceipt {
    let current = s.snapshot().await.unwrap().unwrap();
    s.propose_development(
        &Uuid::new_v4().to_string(),
        current.expected(),
        &propose_refs(pool, d, current.id).await,
        current.id,
        d,
    )
    .await
    .unwrap()
}
async fn refs(pool: &PgPool, id: Uuid, c: &DevelopmentCommand) -> Vec<Uuid> {
    let (target,before_head,draft,proposer,head,scope):(Uuid,Option<Uuid>,Value,Uuid,Option<Uuid>,Option<Uuid>)=sqlx::query_as("SELECT d.target_id,d.before_head,d.draft,d.proposer_id,t.current_revision_id,t.scope_id FROM developments d JOIN development_targets t ON t.individual_id=d.individual_id AND t.id=d.target_id WHERE d.id=$1").bind(id).fetch_one(pool).await.unwrap();
    let draft: DevelopmentDraft = serde_json::from_value(draft).unwrap();
    let mut ids = vec![id, target, proposer];
    ids.extend(before_head);
    ids.extend(head);
    ids.extend(scope);
    ids.extend(draft.evidence_ids);
    ids.extend(draft.counterevidence_ids);
    match c {
        DevelopmentCommand::StartTrial {
            source_event_ids, ..
        } => ids.extend(source_event_ids),
        DevelopmentCommand::RecordValidation {
            review_event_id,
            assessment,
            source_event_ids,
        } => {
            ids.push(*review_event_id);
            ids.push(assessment.reviewer_id);
            let registration: Option<Uuid> = sqlx::query_scalar(
                "SELECT registration_event_id FROM development_verifiers WHERE id=$1",
            )
            .bind(assessment.reviewer_id)
            .fetch_optional(pool)
            .await
            .unwrap();
            ids.extend(registration);
            ids.extend(source_event_ids);
            ids.extend(&assessment.reviewed_evidence_ids);
            ids.extend(&assessment.reviewed_counterevidence_ids);
            ids.extend(&assessment.counter_search_event_ids);
            for o in &assessment.observations {
                ids.extend(&o.source_event_ids);
                ids.push(o.result_event_id)
            }
        }
        _ => {}
    }
    unique(ids)
}
async fn transition(
    s: &Store,
    pool: &PgPool,
    id: Uuid,
    c: &DevelopmentCommand,
) -> DevelopmentReceipt {
    let version: i64 = sqlx::query_scalar("SELECT version FROM developments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    s.transition_development(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &refs(pool, id, c).await,
        id,
        version,
        c,
    )
    .await
    .unwrap()
}
async fn trial(s: &Store, pool: &PgPool, id: Uuid) -> DevelopmentReceipt {
    let draft: Value = sqlx::query_scalar("SELECT draft FROM developments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    let d: DevelopmentDraft = serde_json::from_value(draft).unwrap();
    let mut ids = d.evidence_ids;
    ids.extend(d.counterevidence_ids);
    transition(
        s,
        pool,
        id,
        &DevelopmentCommand::StartTrial {
            route: ValidationRoute::CrossScenarioHabit,
            source_event_ids: ids,
        },
    )
    .await
}
async fn observed(
    pool: &PgPool,
    verifier: Uuid,
    trial: Option<Uuid>,
    role: &str,
    case: Option<Uuid>,
    context: Option<&str>,
    outcome: Option<&str>,
) -> Uuid {
    let payload = json!({"verifier_id":verifier,"trial_event_id":trial,"role":role,"case_id":case,"context":context,"outcome":outcome});
    let id = event(
        pool,
        "development.observed",
        "direct_observation",
        &format!("development-verifier:{verifier}"),
        payload,
    )
    .await;
    sqlx::query("INSERT INTO development_observations(individual_id,event_id,verifier_id,trial_event_id,role,case_id,context,outcome) SELECT id,$1,$2,$3,$4,$5,$6,$7 FROM individuals").bind(id).bind(verifier).bind(trial).bind(role).bind(case).bind(context).bind(outcome).execute(pool).await.unwrap();
    id
}
async fn review(pool: &PgPool, id: Uuid, verifier: Uuid, outcome: Outcome) -> DevelopmentCommand {
    review_contexts(
        pool,
        id,
        verifier,
        outcome,
        &[
            "independent task context",
            "independent relationship context",
        ],
    )
    .await
}
async fn review_contexts(
    pool: &PgPool,
    id: Uuid,
    verifier: Uuid,
    outcome: Outcome,
    contexts: &[&str],
) -> DevelopmentCommand {
    let value: Value = sqlx::query_scalar("SELECT draft FROM developments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    let d: DevelopmentDraft = serde_json::from_value(value).unwrap();
    let trial: Uuid = sqlx::query_scalar(
        "SELECT event_id FROM development_steps WHERE development_id=$1 AND command ? 'StartTrial'",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    let search = observed(
        pool,
        verifier,
        Some(trial),
        "counter_search",
        None,
        None,
        Some("passed"),
    )
    .await;
    let mut observations = vec![];
    let mut source_ids = d.evidence_ids.clone();
    source_ids.extend(&d.counterevidence_ids);
    source_ids.push(search);
    let result = match outcome {
        Outcome::Passed => "passed",
        Outcome::Failed => "failed",
        Outcome::Unverified => "unverified",
    };
    for &context in contexts {
        let case = Uuid::new_v4();
        let result_event_id = observed(
            pool,
            verifier,
            Some(trial),
            "scenario_result",
            Some(case),
            Some(context),
            Some(result),
        )
        .await;
        source_ids.push(result_event_id);
        observations.push(ScenarioObservation {
            case_id: case,
            context: context.into(),
            source_event_ids: d.evidence_ids.clone(),
            result_event_id,
            outcome,
        });
    }
    let a = IndependentAssessment {
        review_id: Uuid::new_v4(),
        reviewer_id: verifier,
        origin: ReviewOrigin::IndependentMeasurement,
        reviewed_evidence_ids: d.evidence_ids,
        reviewed_counterevidence_ids: d.counterevidence_ids,
        counter_search_event_ids: vec![search],
        counter_review: Outcome::Passed,
        observations,
    };
    let review_event=event(pool,"development.review.recorded","direct_observation",&format!("development-verifier:{verifier}"),json!({"development_id":id,"trial_event_id":trial,"review_id":a.review_id,"verifier_id":verifier})).await;
    sqlx::query("INSERT INTO development_review_records(individual_id,event_id,development_id,trial_event_id,verifier_id,assessment) SELECT id,$1,$2,$3,$4,$5 FROM individuals").bind(review_event).bind(id).bind(trial).bind(verifier).bind(serde_json::to_value(&a).unwrap()).execute(pool).await.unwrap();
    for source in &source_ids {
        sqlx::query("INSERT INTO development_review_sources(individual_id,review_event_id,event_id) SELECT id,$1,$2 FROM individuals").bind(review_event).bind(source).execute(pool).await.unwrap();
    }
    DevelopmentCommand::RecordValidation {
        review_event_id: review_event,
        assessment: a,
        source_event_ids: unique(source_ids),
    }
}
async fn accepted(
    s: &Store,
    pool: &PgPool,
    d: &DevelopmentDraft,
    verifier: Uuid,
) -> DevelopmentReceipt {
    let r = propose(s, pool, d).await;
    trial(s, pool, r.development_id).await;
    let review = review(pool, r.development_id, verifier, Outcome::Passed).await;
    transition(s, pool, r.development_id, &review).await;
    transition(s, pool, r.development_id, &DevelopmentCommand::Accept).await
}
async fn fingerprint(pool: &PgPool) -> Value {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut values = serde_json::Map::new();
    for table in tables {
        assert!(table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let hash:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{table}\" t")).fetch_one(pool).await.unwrap();
        values.insert(table, json!(hash));
    }
    Value::Object(values)
}

use alia::{runtime::Runtime, service};
use reqwest::{Client, Method, StatusCode};
use std::path::PathBuf;
const TOKEN: &str = "root-development-http-isolated-not-production";
struct Server {
    rt: Runtime,
    base: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn start(pool: &PgPool) -> Self {
        let rt = Runtime::new(pool.clone(), TOKEN).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let app = service::router(rt.clone(), PathBuf::from("web/dist"));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            rt,
            base,
            client: Client::new(),
            task,
        }
    }
    async fn call(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
        auth: bool,
    ) -> (StatusCode, reqwest::header::HeaderMap, Value) {
        let mut request = self.client.request(
            Method::from_bytes(method.as_bytes()).unwrap(),
            format!("{}{path}", self.base),
        );
        if auth {
            request = request.bearer_auth(TOKEN);
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body.to_owned());
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        assert_eq!(headers["cache-control"], "no-store");
        assert_eq!(headers["x-content-type-options"], "nosniff");
        let value = response.json().await.unwrap();
        (status, headers, value)
    }
    async fn get(&self, path: &str) -> (StatusCode, reqwest::header::HeaderMap, Value) {
        self.call("GET", path, &[], None, true).await
    }
    async fn revert(
        &self,
        id: Uuid,
        version: i64,
        key: &str,
        reason: &str,
    ) -> (StatusCode, reqwest::header::HeaderMap, Value) {
        self.call(
            "POST",
            &format!("/developments/{id}/revert"),
            &[
                ("idempotency-key", key),
                ("if-match", &format!("\"{version}\"")),
            ],
            Some(&json!({"reason":reason}).to_string()),
            true,
        )
        .await
    }
}
