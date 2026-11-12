//! Independent, synthetic semantic checks plus one normal author/reviewer worker.
//! Prewritten candidates are explicitly not paid author output or personal history.
use alia::{
    adapters::{connect_database, model::ChatProvider, store::Store, web::HttpWebReader},
    domain::{
        cognition::DecisionProposal,
        config::{PersonaSeed, RuntimeRules},
        grounding::*,
        history::{MemoryKind, NewMemory},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        model::*,
        scheduling::RunOutcome,
        worker::{Worker, WorkerOutcome},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Notify;
use uuid::Uuid;
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!("Live grounding checks incomplete; provider errors withheld");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("DATABASE_URL")?;
    let parsed = reqwest::Url::parse(&url)?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_live_test_") {
        return Err("test database required".into());
    }
    let pool = connect_database(&url).await?;
    let s = Store::new(pool.clone());
    s.migrate().await?;
    s.initialize(
        "init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json"))?,
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )?,
    )
    .await?;
    let mut memories = Vec::new();
    let mut sources = Vec::new();
    for (i, text) in [
        "顾澄晚饭后喜欢散步；顾橙在这段记录中只是被提及的朋友。",
        "顾橙晚饭后喜欢整理摄影作品；顾澄只是这段记录中被提到的朋友。",
    ]
    .into_iter()
    .enumerate()
    {
        let source = s
            .import_document(
                &format!("source-{i}"),
                "独立合成资料",
                text,
                "controlled source; no gender provided",
            )
            .await?
            .event_id;
        let m = s
            .record_memories(
                &format!("memory-{i}"),
                s.snapshot().await?.ok_or("identity")?.expected(),
                &[source],
                &[NewMemory {
                    content: text.into(),
                    kind: MemoryKind::Semantic,
                    scope: "general".into(),
                    source_event_ids: vec![source],
                    supersedes: None,
                }],
            )
            .await?
            .memory_ids[0];
        memories.push(m);
        sources.push(source);
    }
    let provider = Arc::new(ChatProvider::from_env()?);
    if !matches!(provider.status().readiness, ModelReadiness::Ready) {
        return Err("provider not ready".into());
    }
    let mut stage = "normal_worker";
    let mut current_probe = None;
    let mut reported = false;
    let mut normal = None;
    let mut probes = Vec::new();
    let attempted: Result<(), Box<dyn std::error::Error>> = async {
    let worker = Worker::new(
        s.clone(),
        provider.clone(),
        Arc::new(HttpWebReader),
        Arc::new(Notify::new()),
    );
    let submitted = s
        .submit_message(
            "normal-question",
            "顾澄晚饭后喜欢做什么？请据资料简短回答，不推测未给出的个人信息。",
            Some(Uuid::new_v4()),
        )
        .await?;
    let outcome = worker.run_once().await?;
    let replies:Vec<Value>=sqlx::query_scalar("SELECT payload FROM events WHERE kind='message.sent' AND correlation_id=$1 ORDER BY sequence").bind(submitted.operation_id).fetch_all(&pool).await?;
    let reviews:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('event_id',event_id,'assessment',assessment,'supported',supported,'checker_version',checker_version) FROM decision_reviews WHERE run_id=$1 ORDER BY created_at").bind(submitted.operation_id).fetch_all(&pool).await?;
    let normal_pass = matches!(outcome,WorkerOutcome::Committed(id) if id==submitted.operation_id)
        && !replies.is_empty()
        && reviews.iter().any(|r| r["supported"] == true);
    let diagnostics:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('kind',kind,'payload',payload) FROM events WHERE correlation_id=$1 AND kind IN ('run.retry_scheduled','run.stopped','tool.result') ORDER BY sequence").bind(submitted.operation_id).fetch_all(&pool).await?;
    normal = Some(json!({"outcome":format!("{outcome:?}"),"replies":replies,"reviews":reviews,"diagnostics":diagnostics,"engineering_pass":normal_pass}));
    for (id, text, wants_supported) in [
        ("supported", "顾澄晚饭后喜欢散步。", true),
        ("attribute", "顾澄是女性，晚饭后喜欢散步。", false),
        (
            "uncited_other_person",
            "顾澄晚饭后喜欢散步，顾橙喜欢整理摄影作品。",
            false,
        ),
        (
            "parenthetical_leak",
            "根据资料，顾澄晚饭后喜欢散步。（另有一条记录说的是顾橙喜欢整理摄影作品，是不同的人，未混淆。）",
            false,
        ),
        (
            "atomic_memory_goal",
            "已记住顾澄晚饭后喜欢散步，并保留了一个了解散步资料的研究目标。",
            true,
        ),
        (
            "atomic_write_not_external_send",
            "已记住顾澄晚饭后喜欢散步，并已把研究报告发送给顾澄。",
            false,
        ),
        (
            "unwritten_new_memory",
            "我刚刚把顾澄晚饭后喜欢散步另存为了一条新的长期记忆。",
            false,
        ),
        (
            "created_goal_not_completed",
            "已保留了解散步资料的研究目标，并已完成该目标、写好了研究笔记。",
            false,
        ),
        (
            "unread_document_metadata",
            "已记住顾澄晚饭后喜欢散步。",
            false,
        ),
    ] {
        current_probe = Some(id);
        stage = "probe_setup";
        let has_statement = matches!(id,"atomic_memory_goal"|"atomic_write_not_external_send"|"created_goal_not_completed"|"unwritten_new_memory");
        let submitted = s.submit_message(
            &format!("probe-{id}"),
            if has_statement {"这是独立审查试验；本次合成资料明确陈述：顾澄晚饭后喜欢散步。候选内容不是已经发布的回复。"} else {"这是独立审查试验；候选内容不是已经发布的回复。"},
            Some(Uuid::new_v4()),
        )
        .await?;
        let lease = s.claim_run(Uuid::new_v4()).await?.ok_or("probe lease")?;
        let c = s
            .model_context(lease.run_id, s.local_capabilities().await?)
            .await?;
        let memory_changes = if id == "attribute" {
            json!([{"content":text,"kind":"semantic","scope":"general","source_event_ids":[sources[0]],"supersedes":null}])
        } else if matches!(id, "atomic_memory_goal" | "atomic_write_not_external_send") {
            json!([{"content":"顾澄晚饭后喜欢散步。","kind":"semantic","scope":"general","source_event_ids":[submitted.event_id],"supersedes":null}])
        } else if id == "unread_document_metadata" {
            // Deliberately insufficient: import metadata is not document text.
            json!([{"content":"顾澄晚饭后喜欢散步。","kind":"semantic","scope":"general","source_event_ids":[sources[0]],"supersedes":null}])
        } else {
            json!([])
        };
        let goal_changes = if matches!(id, "atomic_memory_goal" | "created_goal_not_completed") {
            json!([{"action":"create","origins":[{"origin_type":"event","origin_id":submitted.event_id}],"description":"了解散步资料","success_criterion":"实际阅读后写出有来源的研究笔记","next_step":"寻找允许读取的资料","stop_condition":"缺资料则等待","priority":50}])
        } else {
            json!([])
        };
        let reply_source = if has_statement {submitted.event_id} else {memories[0]};
        let candidate=DecisionProposal::parse(json!({"schema_version":1,"reply":text,"reply_evidence_ids":[reply_source],"memory_changes":memory_changes,"goal_changes":goal_changes,"tool_intents":[],"development":null,"brief_reason":"受控候选，仅供审查"}).to_string().as_bytes())?;
        stage = "probe_candidate_validation";
        candidate.validate_context(&c)?;
        let c = review_context(&c, Uuid::new_v4(), &candidate)?;
        let limits = CallLimits::default();
        let allowance = provider.decision_review_allowance(&c, limits)?;
        let reservation = s
            .reserve_model_call(&lease, Uuid::new_v4(), &c, allowance)
            .await?;
        if !s.dispatch_model_call(&lease, reservation.id).await? {
            return Err("dispatch replay".into());
        }
        stage = "probe_review";
        let result = provider
            .review_decision(&c, limits, ModelCallMetadata::foreground(&c)?)
            .await;
        let usage = match &result {
            Ok(r) => r.usage,
            Err(e) => e.usage,
        };
        s.settle_model_call(reservation.id, usage).await?;
        let r = match result {
            Ok(r) => r,
            Err(error) => {
                // Invalid review output is a failed case, not a reason to retry
                // until green or discard preceding and remaining evidence.
                probes.push(json!({"id":id,"candidate_origin":"prewritten controlled candidate; no author model call","candidate":candidate,"expected_supported":wants_supported,"passed":false,"error_code":format!("{:?}",error.error),"usage":error.usage,"input_bound":allowance.input_upper_bound,"output_cap":allowance.output_cap}));
                s.finish_run(&lease, RunOutcome::Cancelled).await?;
                continue;
            }
        };
        r.assessment.validate(&c)?;
        let passed = r.assessment.supported() == wants_supported
            && r.usage.is_some_and(|u| {
                u.input <= allowance.input_upper_bound
                    && u.output <= u64::from(allowance.output_cap)
            });
        probes.push(json!({"id":id,"candidate_origin":"prewritten controlled candidate; no author model call; parenthetical_leak replays a prior observed failure","candidate":candidate,"assessment":r.assessment,"expected_supported":wants_supported,"passed":passed,"usage":r.usage,"input_bound":allowance.input_upper_bound,"output_cap":allowance.output_cap,"mode":r.provider_mode,"provider_model":r.provider_model}));
        // Benchmarks only observe assessments; they cannot commit candidate state.
        s.finish_run(&lease, RunOutcome::Cancelled).await?;
    }
    stage = "final_summary";
    let usage:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('run_id',run_id,'input',actual_input,'output',actual_output,'charged',charged_tokens,'unknown',usage_unknown,'status',status,'input_bound',input_bound,'output_cap',output_cap) FROM usage_reservations ORDER BY created_at").fetch_all(&pool).await?;
    let state = s.snapshot().await?.ok_or("identity")?;
    let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals WHERE singleton")
        .fetch_one(&pool)
        .await?;
    let final_memories:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'content',content,'validity',validity,'scope',scope) FROM memories ORDER BY created_at").fetch_all(&pool).await?;
    let pass = normal_pass
        && !frozen
        && probes.len() == 9
        && probes.iter().all(|p| p["passed"] == true)
        && usage
            .iter()
            .all(|u| u["status"] == "settled" && u["unknown"] == false);
    reported = true;
    println!(
        "{}",
        json!({"phase":"grounding","packet_version":PACKET_VERSION,"individual_id":state.id,"data":"synthetic development evidence; real model reviewer and normal worker; controlled benchmark candidates never committed","normal_worker":normal,"probes":probes,"usage":usage,"final_memories":final_memories,"budget_frozen":frozen,"engine_pass":pass,"behavior_review":"pending manual review; small cases do not prove general semantic accuracy"})
    );
    if pass {
        Ok(())
    } else {
        Err("semantic development checks failed".into())
    }
    }.await;
    if let Err(error) = &attempted
        && !reported
    {
        // A failed probe must not discard real paid usage when its disposable
        // database is removed. Never print provider bodies or database errors.
        let code = error
            .downcast_ref::<alia::domain::cognition::ProposalError>()
            .map(|e| e.feedback().0.to_owned())
            .or_else(|| {
                error
                    .downcast_ref::<ModelFailure>()
                    .map(|e| format!("{:?}", e.error))
            })
            .unwrap_or_else(|| "test_driver_failure".into());
        let usage:Result<Vec<Value>,_>=sqlx::query_scalar("SELECT jsonb_build_object('run_id',run_id,'input',actual_input,'output',actual_output,'charged',charged_tokens,'unknown',usage_unknown,'status',status,'input_bound',input_bound,'output_cap',output_cap) FROM usage_reservations ORDER BY created_at").fetch_all(&pool).await;
        println!(
            "{}",
            json!({"phase":"grounding","packet_version":PACKET_VERSION,"stage":stage,"probe_id":current_probe,"error_code":code,"normal_worker":normal,"probes":probes,"usage":usage.as_ref().ok(),"accounting_recovered":usage.is_ok(),"data":"synthetic development failure; prior usage retained, no raw provider output","engine_pass":false,"budget_frozen":null})
        );
    }
    pool.close().await;
    attempted
}
