//! Provenance and execution contracts, not evidence of model semantic quality.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        beliefs::{BeliefDraft, BeliefPosition, CurrentBelief, SelectedBeliefs},
        cognition::{DecisionProposal, ModelContext, ProposalError},
        config::{PersonaSeed, RuntimeRules},
        grounding::{self, AssessmentError},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn sample() -> ModelContext {
    serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap()
}
fn belief(id: Uuid, support: Uuid, counter: Uuid) -> CurrentBelief {
    CurrentBelief {
        id,
        version: 1,
        statement: "一般先研究，但不是所有任务都适用".into(),
        scope: "general".into(),
        position: BeliefPosition::Questioned,
        brief_basis: "保留原先陈述和本次明确例外".into(),
        support_event_ids: vec![support],
        counter_event_ids: vec![counter],
    }
}
fn selected() -> ModelContext {
    let mut c = sample();
    let id = Uuid::new_v4();
    c.selected_beliefs = Some(SelectedBeliefs {
        items: vec![belief(id, c.events[0].id, c.events[1].id)],
        has_more: false,
    });
    c.input_manifest.push(id);
    c
}
fn proposal(id: Uuid) -> DecisionProposal {
    serde_json::from_value(json!({"schema_version":1,"reply":"我目前的理解需要保留例外。","reply_evidence_ids":[id],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"立场不等于事实证明"})).unwrap()
}

#[test]
fn incomplete_foreign_scope_or_unmanifested_beliefs_cannot_enter_a_decision() {
    let c = sample();
    assert!(c.selected_beliefs.is_none());
    assert!(
        serde_json::to_value(&c)
            .unwrap()
            .get("selected_beliefs")
            .is_none()
    );
    let c = selected();
    let id = c.selected_beliefs.as_ref().unwrap().items[0].id;
    assert!(proposal(id).validate_context(&c).is_ok());
    let counter = c.selected_beliefs.as_ref().unwrap().items[0].counter_event_ids[0];
    let mut missing = c.clone();
    missing.events.retain(|e| e.id != counter);
    assert_eq!(
        proposal(id).validate_context(&missing),
        Err(ProposalError::UnknownEvidence)
    );
    let mut unmanifested = c.clone();
    unmanifested.input_manifest.retain(|e| *e != counter);
    assert_eq!(
        proposal(id).validate_context(&unmanifested),
        Err(ProposalError::UnknownEvidence)
    );
    let mut other_scope = c.clone();
    other_scope.selected_beliefs.as_mut().unwrap().items[0].scope = "event:another-task".into();
    assert_eq!(
        proposal(id).validate_context(&other_scope),
        Err(ProposalError::UnknownEvidence)
    );
    let mut unknown = c;
    unknown.selected_beliefs.as_mut().unwrap().items[0].version = 0;
    assert_eq!(
        proposal(id).validate_context(&unknown),
        Err(ProposalError::UnknownEvidence)
    );
}

#[test]
fn belief_citation_cannot_be_laundered_into_memory_event_evidence() {
    let c = selected();
    let id = c.selected_beliefs.as_ref().unwrap().items[0].id;
    let mut p = proposal(id);
    p.memory_changes = serde_json::from_value(json!([{"content":"新的解释","kind":"semantic","scope":"general","source_event_ids":[id],"supersedes":null}])).unwrap();
    assert_eq!(p.validate_context(&c), Err(ProposalError::UnknownEvidence));
    let review = grounding::review_context(&c, Uuid::new_v4(), &p).unwrap();
    assert!(matches!(
        grounding::evidence_packet(&review),
        Err(AssessmentError::Evidence)
    ));
}

#[test]
fn reviewer_retains_counterevidence_without_expanding_declared_citations() {
    let c = selected();
    let b = &c.selected_beliefs.as_ref().unwrap().items[0];
    let p = proposal(b.id);
    let review = grounding::review_context(&c, Uuid::new_v4(), &p).unwrap();
    let packet = grounding::evidence_packet(&review).unwrap();
    let target = &packet["evidence_by_target"][0];
    assert_eq!(target["allowed_evidence_ids"], json!([b.id]));
    let item = &target["evidence"][0];
    assert_eq!(item["type"], "belief");
    assert_eq!(item["proof_scope"], "individual_stance_not_world_fact");
    let basis = item["basis_events"].as_array().unwrap();
    assert!(
        basis
            .iter()
            .any(|e| e["record"]["id"] == json!(b.counter_event_ids[0])
                && e["roles"] == json!(["counter"]))
    );
    assert!(
        basis
            .iter()
            .any(|e| e["record"]["id"] == json!(b.support_event_ids[0])
                && e["roles"] == json!(["support"]))
    );
    let a: grounding::DecisionAssessment = serde_json::from_value(json!({"schema_version":1,"checks":[{"target":{"kind":"reply"},"verdict":"supported","reason":"只声明当前立场","evidence_ids":[b.id]}]})).unwrap();
    assert!(a.validate(&review).is_ok());
    let mut borrowed = a;
    borrowed.checks[0].evidence_ids = b.counter_event_ids.clone();
    assert!(
        borrowed.validate(&review).is_err(),
        "nested sources are not automatic declared citations"
    );
}

async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    s
}
async fn save(s: &Store, key: &str, draft: BeliefDraft) -> Uuid {
    let mut inputs: Vec<_> = draft
        .support_event_ids
        .iter()
        .chain(&draft.counter_event_ids)
        .copied()
        .collect();
    inputs.extend(draft.supersedes);
    inputs.sort();
    inputs.dedup();
    s.record_belief(
        key,
        s.snapshot().await.unwrap().unwrap().expected(),
        &inputs,
        &draft,
    )
    .await
    .unwrap()
    .belief_id
}
fn draft(support: Vec<Uuid>, counter: Vec<Uuid>) -> BeliefDraft {
    BeliefDraft {
        statement: "当前认识保留例外".into(),
        scope: "general".into(),
        position: BeliefPosition::Questioned,
        support_event_ids: support,
        counter_event_ids: counter,
        supersedes: None,
        brief_basis: "以实际收到的陈述为依据".into(),
    }
}
async fn context(s: &Store, pool: &PgPool, trigger: Uuid) -> ModelContext {
    context_and_lease(s, pool, trigger).await.0
}
async fn context_and_lease(s: &Store, pool: &PgPool, trigger: Uuid) -> (ModelContext, RunLease) {
    // Fixture isolation: leave just the designated real submitted run claimable.
    sqlx::query(
        "UPDATE runs SET status='cancelled' WHERE status='queued' AND trigger_event_id<>$1",
    )
    .bind(trigger)
    .execute(pool)
    .await
    .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let actual: Uuid = sqlx::query_scalar("SELECT trigger_event_id FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(actual, trigger);
    (s.model_context(lease.run_id, vec![]).await.unwrap(), lease)
}
async fn state_counts(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('individuals',(SELECT jsonb_agg(to_jsonb(i)) FROM individuals i),'events',(SELECT count(*) FROM events),'beliefs',(SELECT count(*) FROM beliefs),'outbox',(SELECT count(*) FROM outbox),'usage',(SELECT count(*) FROM usage_reservations))")
        .fetch_one(pool).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn context_restores_current_stance_and_old_support_and_counter_sources(pool: PgPool) {
    let s = setup(&pool).await;
    let support = s
        .submit_message("source", "一般先研究", None)
        .await
        .unwrap()
        .event_id;
    let old = save(&s, "first", draft(vec![support], vec![])).await;
    let counter = s
        .submit_message("counter", "原先的一般说法不覆盖所有情况", None)
        .await
        .unwrap()
        .event_id;
    let mut next = draft(vec![support], vec![counter]);
    next.supersedes = Some(old);
    let revised = save(&s, "revision", next).await;
    let mut unrelated = draft(vec![support], vec![]);
    unrelated.scope = "event:unrelated".into();
    let unrelated = save(&s, "unrelated", unrelated).await;
    for i in 0..10 {
        s.submit_message(&format!("later-{i}"), "后续不相关输入", None)
            .await
            .unwrap();
    }
    let input = s
        .submit_message("current", "这次先做原型", None)
        .await
        .unwrap();
    let mut local = draft(vec![input.event_id], vec![]);
    local.scope = format!("event:{}", input.event_id);
    let local = save(&s, "local", local).await;
    let c = context(&Store::new(pool.clone()), &pool, input.event_id).await;
    let beliefs = c.selected_beliefs.as_ref().unwrap();
    assert!(beliefs.valid_for(&c));
    assert!(!beliefs.has_more);
    assert_eq!(beliefs.items.len(), 2);
    assert_eq!(beliefs.items[0].id, local);
    assert!(beliefs.items.iter().any(|b| b.id == revised));
    // A predecessor now belongs to the explicit historical basis, never the
    // selection of current stances. An unrelated scope must still stay absent.
    assert!(!beliefs.items.iter().any(|b| b.id == old));
    let historical = c.autobiographical_context.as_ref().unwrap();
    assert!(
        historical
            .belief_basis
            .iter()
            .any(|b| b.id == old && b.status == "superseded")
    );
    assert!(c.input_manifest.contains(&old));
    assert!(!c.input_manifest.contains(&unrelated));
    assert!(!historical.belief_basis.iter().any(|b| b.id == unrelated));
    for present in [revised, local, support, counter] {
        assert!(c.input_manifest.contains(&present));
    }
    assert!(
        c.events
            .iter()
            .any(|e| e.id == support && e.payload["text"] == "一般先研究")
    );
    assert!(
        c.events
            .iter()
            .any(|e| e.id == counter && e.payload["text"] == "原先的一般说法不覆盖所有情况")
    );
    let before = state_counts(&pool).await;
    let again = Store::new(pool.clone())
        .model_context(c.run_id, vec![])
        .await
        .unwrap();
    assert_eq!(again.selected_beliefs.unwrap().items.len(), 2);
    assert_eq!(
        before,
        state_counts(&pool).await,
        "context reads do not create life events or model fees"
    );
    save(&s, "new-belief", draft(vec![support], vec![])).await;
    assert!(matches!(
        s.model_context(c.run_id, vec![]).await,
        Err(StoreError::StaleState)
    ));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn hidden_ancestor_removes_stance_and_its_nonrecent_basis(pool: PgPool) {
    let s = setup(&pool).await;
    let ancestor = s
        .submit_message("ancestor", "待撤销源", None)
        .await
        .unwrap()
        .event_id;
    let relay = s
        .submit_message("relay", "中间资料", None)
        .await
        .unwrap()
        .event_id;
    let source = s
        .submit_message("source", "派生陈述", None)
        .await
        .unwrap()
        .event_id;
    let id = save(&s, "belief", draft(vec![source], vec![])).await;
    for (from, to) in [(ancestor, relay), (relay, source)] {
        sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT individual_id,$1,$2 FROM events WHERE id=$1")
            .bind(from).bind(to).execute(&pool).await.unwrap();
    }
    for i in 0..10 {
        s.submit_message(&format!("later-{i}"), "无关输入", None)
            .await
            .unwrap();
    }
    let trigger = s
        .submit_message("current", "当前任务", None)
        .await
        .unwrap()
        .event_id;
    let c = context(&s, &pool, trigger).await;
    assert!(
        c.selected_beliefs
            .as_ref()
            .unwrap()
            .items
            .iter()
            .any(|b| b.id == id)
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(ancestor)
        .execute(&pool)
        .await
        .unwrap();
    let hidden = s.model_context(c.run_id, vec![]).await.unwrap();
    assert!(hidden.selected_beliefs.is_none());
    for private in [id, source, ancestor, relay] {
        assert!(!hidden.input_manifest.contains(&private));
    }
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn context_budget_omits_a_whole_large_bundle_including_counterevidence(pool: PgPool) {
    let s = setup(&pool).await;
    let small = s
        .submit_message("small", "少量来源", None)
        .await
        .unwrap()
        .event_id;
    let retained = save(&s, "small-belief", draft(vec![small], vec![])).await;
    let mut large = Vec::new();
    for i in 0..14 {
        large.push(
            s.submit_message(&format!("big-{i}"), &"资".repeat(5000), None)
                .await
                .unwrap()
                .event_id,
        );
    }
    let omitted = save(
        &s,
        "large-belief",
        draft(large[..7].to_vec(), large[7..].to_vec()),
    )
    .await;
    for i in 0..10 {
        s.submit_message(&format!("later-{i}"), "无关输入", None)
            .await
            .unwrap();
    }
    let trigger = s
        .submit_message("current", "当前任务", None)
        .await
        .unwrap()
        .event_id;
    let c = context(&s, &pool, trigger).await;
    let selected = c.selected_beliefs.as_ref().unwrap();
    assert!(selected.has_more);
    assert_eq!(selected.items.len(), 1);
    assert_eq!(selected.items[0].id, retained);
    assert!(!c.input_manifest.contains(&omitted));
    for id in large {
        assert!(!c.input_manifest.contains(&id));
        assert!(!c.events.iter().any(|e| e.id == id));
    }
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn revocation_between_selection_reservation_and_dispatch_never_sends_private_basis(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let ancestor = s
        .submit_message("ancestor", "上游资料", None)
        .await
        .unwrap()
        .event_id;
    let source = s
        .submit_message("source", "根据上游形成的陈述", None)
        .await
        .unwrap()
        .event_id;
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT individual_id,$1,$2 FROM events WHERE id=$1")
        .bind(ancestor).bind(source).execute(&pool).await.unwrap();
    save(&s, "belief", draft(vec![source], vec![])).await;
    let trigger = s
        .submit_message("current", "当前任务", None)
        .await
        .unwrap()
        .event_id;
    let (c, lease) = context_and_lease(&s, &pool, trigger).await;
    let allowance = || TokenAllowance {
        input_upper_bound: 1000,
        output_cap: 32,
    };
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(ancestor)
        .execute(&pool)
        .await
        .unwrap();
    let before = state_counts(&pool).await;
    assert!(matches!(
        s.reserve_model_call(&lease, Uuid::new_v4(), &c, allowance())
            .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(before, state_counts(&pool).await);
    sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
        .bind(ancestor)
        .execute(&pool)
        .await
        .unwrap();
    let reserved = s
        .reserve_model_call(&lease, Uuid::new_v4(), &c, allowance())
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(ancestor)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.dispatch_model_call(&lease, reserved.id).await,
        Err(StoreError::LeaseLost)
    ));
    let (status, sent, calls): (String, bool, i32) = sqlx::query_as("SELECT u.status,u.dispatched_at IS NOT NULL,r.model_calls FROM usage_reservations u JOIN runs r ON r.id=u.run_id WHERE u.id=$1")
        .bind(reserved.id).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "reserved");
    assert!(!sent);
    assert_eq!(calls, 0);
}
