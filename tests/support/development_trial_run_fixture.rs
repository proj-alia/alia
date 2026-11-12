// Shared actual Store/plan/queue fixture; all inputs are explicitly synthetic.
fn no_changes() -> Value {
    json!({"schema_version":1,"reply":"","reply_evidence_ids":[],
        "memory_changes":[],"goal_changes":[],"tool_intents":[],
        "development":null,"brief_reason":"明确合成工程用例"})
}

async fn work(pool: &PgPool) -> (Store, RunLease, TrialPlan, Uuid) {
    let s = setup(pool).await;
    let input = s
        .submit_message("goal-input", "请保存一个有来源的研究目标", None)
        .await
        .unwrap();
    let author = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut proposal = no_changes();
    proposal["goal_changes"] = json!([{"action":"create","origin_type":"event",
        "origin_ids":[input.event_id],"description":"真实存储的工程目标",
        "success_criterion":"形成可核查的发现","next_step":"检查资料",
        "stop_condition":"资料缺失时等待","priority":30}]);
    let prepared = model_step(
        &s,
        &FixtureProvider {
            response: proposal.to_string(),
        },
        &author,
        s.capabilities().await.unwrap(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    let goal = s
        .commit_decision(&author, &prepared, &s.capabilities().await.unwrap())
        .await
        .unwrap()
        .goal_ids[0];
    // Future due date only isolates this trial test from unrelated normal work.
    sqlx::query("UPDATE goals SET due_at=clock_timestamp()+interval '1 day' WHERE id=$1")
        .bind(goal)
        .execute(pool)
        .await
        .unwrap();
    let mut sources = vec![input.event_id];
    for _ in 0..2 {
        sources.push(message(&s).await);
    }
    while let Some(queued) = s.claim_run(Uuid::new_v4()).await.unwrap() {
        s.finish_run(&queued, alia::runtime::scheduling::RunOutcome::Cancelled)
            .await
            .unwrap();
    }
    let mut candidate = draft(
        sources[0],
        DevelopmentTarget::TaskStrategy,
        json!({"scope":{"kind":"goal","id":goal},"description":"先检查来源，再汇总发现"}),
    );
    candidate.evidence_ids = sources.clone();
    let proposed = propose(&s, pool, &candidate).await;
    let evaluator = s
        .register_development_measurement("program", MeasurementProgram::JsonComparisonV1)
        .await
        .unwrap();
    let manifest: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM objects WHERE NOT blocked ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    let cases = (0..3)
        .map(|index| TrialCase {
            id: Uuid::new_v4(),
            context: format!("independent fixture context {index}"),
            prompt: [
                "受控案例：请区分一条陈述及其证据来源。",
                "受控案例：两份资料互相矛盾时，请说明尚不能确定的部分。",
                "受控反例：没有已知共同经历时，不得补造过去。",
            ][index]
                .into(),
            role: if index == 2 {
                TrialCaseRole::CounterSearch
            } else {
                TrialCaseRole::Scenario
            },
            criterion: TrialCriterion::JsonEquals {
                pointer: "/reply".into(),
                expected: json!("fixture criterion"),
            },
            source_event_ids: vec![sources[index]],
        })
        .collect();
    let plan = s
        .freeze_development_trial_plan(
            "plan",
            s.snapshot().await.unwrap().unwrap().expected(),
            &manifest,
            proposed.development_id,
            proposed.development_version,
            evaluator,
            ValidationRoute::CrossScenarioHabit,
            &TrialPlanDraft {
                cases,
                deadline: Utc::now() + Duration::hours(1),
            },
        )
        .await
        .unwrap();
    let run = s
        .enqueue_development_trial_case("enqueue", plan.id, plan.cases[0].id)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(lease.run_id, run);
    (s, lease, plan, goal)
}
