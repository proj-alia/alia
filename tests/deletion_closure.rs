//! Actual isolated PostgreSQL dependency closures, including deliberately absent derivation edges.
use alia::{
    adapters::store::{Store, StoreError},
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
async fn setup(pool: &PgPool) -> Store {
    let store = Store::new(pool.clone());
    store
        .initialize(
            "p05-delete-fixture",
            &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
            &RuntimeRules::validate(
                serde_json::from_slice(include_bytes!("../config/runtime.example.json")).unwrap(),
                IMPLEMENTED_CAPABILITIES,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store
}
async fn individual(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM individuals")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn object(pool: &PgPool, kind: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,$2 FROM individuals")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    id
}
async fn event(pool: &PgPool, text: &str) -> Uuid {
    let id = object(pool, "event").await;
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,$1,'message.received','reported_claim','owner',now(),jsonb_build_object('text',$2::text),$1 FROM individuals").bind(id).bind(text).execute(pool).await.unwrap();
    id
}
async fn memory(pool: &PgPool, source: &[Uuid]) -> Uuid {
    let id = object(pool, "memory").await;
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic','PRIVATE-MEMORY-TEXT','general' FROM individuals").bind(id).execute(pool).await.unwrap();
    for e in source {
        sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals").bind(id).bind(e).execute(pool).await.unwrap();
    }
    id
}
async fn edge(pool: &PgPool, source: Uuid, target: Uuid) {
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,$1,$2 FROM individuals ON CONFLICT DO NOTHING").bind(source).bind(target).execute(pool).await.unwrap();
}
async fn blocked(pool: &PgPool, id: Uuid, value: bool) {
    sqlx::query("UPDATE objects SET blocked=$2 WHERE id=$1")
        .bind(id)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
}
async fn belief(pool: &PgPool, source: Uuid, role: &str, predecessor: Option<Uuid>) -> Uuid {
    let id = object(pool, "belief").await;
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis,supersedes) SELECT id,$1,'PRIVATE-BELIEF-TEXT','general','questioned','bounded fixture',$2 FROM individuals").bind(id).bind(predecessor).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) SELECT id,$1,$2,$3 FROM individuals").bind(id).bind(source).bind(role).execute(pool).await.unwrap();
    id
}
async fn autobiography(pool: &PgPool, topic: &str, source: Uuid, stance: Option<Uuid>) -> Uuid {
    let id = object(pool, "autobiography").await;
    sqlx::query("INSERT INTO autobiographies(individual_id,id,topic,title,interpretation) SELECT id,$1,$2,'private title','PRIVATE-INTERPRETATION-TEXT' FROM individuals").bind(id).bind(topic).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO autobiography_events(individual_id,autobiography_id,event_id) SELECT id,$1,$2 FROM individuals").bind(id).bind(source).execute(pool).await.unwrap();
    if let Some(stance) = stance {
        sqlx::query("INSERT INTO autobiography_beliefs(individual_id,autobiography_id,belief_id) SELECT id,$1,$2 FROM individuals").bind(id).bind(stance).execute(pool).await.unwrap();
    }
    id
}
async fn fingerprint(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('individuals',(SELECT md5(jsonb_agg(to_jsonb(i) ORDER BY id)::text) FROM individuals i),'objects',(SELECT md5(jsonb_agg(to_jsonb(o) ORDER BY id)::text) FROM objects o),'events',(SELECT md5(jsonb_agg(to_jsonb(e) ORDER BY sequence)::text) FROM events e),'memories',(SELECT md5(jsonb_agg(to_jsonb(m) ORDER BY id)::text) FROM memories m),'sources',(SELECT md5(jsonb_agg(to_jsonb(s) ORDER BY memory_id,event_id)::text) FROM memory_sources s),'beliefs',(SELECT md5(jsonb_agg(to_jsonb(b) ORDER BY id)::text) FROM beliefs b),'autobiographies',(SELECT md5(jsonb_agg(to_jsonb(a) ORDER BY id)::text) FROM autobiographies a),'outbox',(SELECT count(*) FROM outbox),'usage',(SELECT count(*) FROM usage_reservations),'notes',(SELECT count(*) FROM notes),'runs',(SELECT count(*) FROM runs),'actions',(SELECT count(*) FROM actions))").fetch_one(pool).await.unwrap()
}
#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_multiple_sources_actual_fks_counter_predecessors_history_independent_and_readonly(
    pool: PgPool,
) {
    let store = setup(&pool).await;
    let a = event(&pool, "ROOT-A-PRIVATE").await;
    let b = event(&pool, "ROOT-B-PRIVATE").await;
    let other = event(&pool, "INDEPENDENT-PRIVATE").await;
    let target = memory(&pool, &[a, b]).await;
    let sibling = memory(&pool, &[a]).await;
    let safe = memory(&pool, &[other]).await;
    let stance = belief(&pool, a, "counter", None).await;
    let revision = belief(&pool, other, "support", Some(stance)).await;
    let story = autobiography(&pool, "affected", other, Some(revision)).await;
    let safe_story = autobiography(&pool, "independent", other, None).await;
    let subject = object(&pool, "relationship").await;
    sqlx::query("INSERT INTO relationships(individual_id,subject_id,subject_key) SELECT id,$1,'closure-fixture' FROM individuals").bind(subject).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT id,$1,$2 FROM individuals").bind(subject).bind(b).execute(&pool).await.unwrap();
    let changed = event(&pool, "revision event").await;
    sqlx::query(
        "UPDATE events SET payload=jsonb_build_object('previous_memory_id',$2::uuid) WHERE id=$1",
    )
    .bind(changed)
    .bind(target)
    .execute(&pool)
    .await
    .unwrap();
    let successor = memory(&pool, &[changed]).await;
    sqlx::query("UPDATE memories SET supersedes=$2 WHERE id=$1")
        .bind(successor)
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    edge(&pool, stance, sibling).await;
    edge(&pool, sibling, stance).await;
    let before = fingerprint(&pool).await;
    let got = store.memory_deletion_closure(target, 1).await.unwrap();
    let mut roots = vec![a, b];
    roots.sort();
    assert_eq!(got.root_event_ids, roots);
    assert_eq!(got.target_memory_id, target);
    assert_eq!(got.individual_id, individual(&pool).await);
    assert_eq!(got.memory_version, 1);
    let mut sorted = got.object_ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, got.object_ids);
    for id in [
        a, b, target, sibling, stance, revision, story, subject, changed, successor,
    ] {
        assert!(got.object_ids.contains(&id), "missing {id}");
    }
    for id in [other, safe, safe_story, individual(&pool).await] {
        assert!(!got.object_ids.contains(&id));
    }
    assert_eq!(fingerprint(&pool).await, before);
    let restored = Store::new(pool.clone());
    assert_eq!(
        restored.memory_deletion_closure(target, 1).await.unwrap(),
        got
    );
    assert!(!serde_json::to_string(&got).unwrap().contains("PRIVATE"));
}
#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_missing_edges_full_manifest_run_action_results_notes_and_citations(pool: PgPool) {
    let store = setup(&pool).await;
    let root = event(&pool, "private root").await;
    let target = memory(&pool, &[root]).await;
    let independent = event(&pool, "independent trigger").await;
    let run = object(&pool, "run").await;
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,input_manifest) SELECT id,$1,$2,'foreground',state_version,privacy_epoch,config_version,$3 FROM individuals").bind(run).bind(independent).bind(vec![target]).execute(&pool).await.unwrap();
    let decision = event(&pool, "decision").await;
    let result = event(&pool, "result").await;
    let reply = event(&pool, "reply").await;
    sqlx::query("UPDATE events SET correlation_id=$2 WHERE id IN ($1,$3)")
        .bind(decision)
        .bind(run)
        .bind(reply)
        .execute(&pool)
        .await
        .unwrap();
    let action = object(&pool, "action").await;
    sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments,status,result_event_id) SELECT id,$1,$2,$3,1,'notes.write','1','{}','succeeded',$4 FROM individuals").bind(action).bind(run).bind(decision).bind(result).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO notes(individual_id,action_id,title,body,citations) SELECT id,$1,'note','PRIVATE-NOTE',jsonb_build_array(jsonb_build_object('document_id',$2::uuid,'start_char',0,'end_char',1)) FROM individuals").bind(action).bind(root).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    let got = store.memory_deletion_closure(target, 1).await.unwrap();
    for id in [target, root, run, decision, result, reply, action] {
        assert!(got.object_ids.contains(&id), "missing {id}");
    }
    assert!(!got.object_ids.contains(&independent));
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_person_reference_causation_and_autobiography_supersession_without_edges(
    pool: PgPool,
) {
    let store = setup(&pool).await;
    let root = event(&pool, "root").await;
    let independent = event(&pool, "independent").await;
    let target = memory(&pool, &[root]).await;
    let entity = object(&pool, "entity").await;
    sqlx::query("INSERT INTO person_entities(individual_id,id,external_key,label) SELECT id,$1,'p05-entity','PRIVATE-NAME' FROM individuals").bind(entity).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO person_aliases(individual_id,entity_id,surface,source_event_id,assertion_event_id) SELECT id,$1,'PRIVATE-ALIAS',$2,$2 FROM individuals").bind(entity).bind(root).execute(&pool).await.unwrap();
    let related = memory(&pool, &[independent]).await;
    sqlx::query("INSERT INTO memory_person_links(individual_id,memory_id,memory_version,content_digest,entity_id,role,source_event_id,assertion_event_id) SELECT id,$1,1,$2,$3,'mentioned',$4,$4 FROM individuals").bind(related).bind("a".repeat(64)).bind(entity).bind(independent).execute(&pool).await.unwrap();
    let caused = event(&pool, "caused").await;
    sqlx::query("UPDATE events SET causation_id=$2 WHERE id=$1")
        .bind(caused)
        .bind(root)
        .execute(&pool)
        .await
        .unwrap();
    let history = autobiography(&pool, "first", root, None).await;
    let newer = autobiography(&pool, "second", independent, None).await;
    sqlx::query("UPDATE autobiographies SET supersedes=$2 WHERE id=$1")
        .bind(newer)
        .bind(history)
        .execute(&pool)
        .await
        .unwrap();
    let got = store.memory_deletion_closure(target, 1).await.unwrap();
    for id in [root, target, entity, related, caused, history, newer] {
        assert!(got.object_ids.contains(&id), "missing {id}");
    }
    assert!(!got.object_ids.contains(&independent));
}
#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_version_unknown_hidden_sources_identity_and_foreign_refs_fail_closed(
    pool: PgPool,
) {
    let store = setup(&pool).await;
    let root = event(&pool, "root").await;
    let target = memory(&pool, &[root]).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 2).await,
        Err(StoreError::StaleState)
    ));
    assert!(matches!(
        store.memory_deletion_closure(target, 0).await,
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.memory_deletion_closure(Uuid::new_v4(), 1).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store
            .memory_deletion_closure(individual(&pool).await, 1)
            .await,
        Err(StoreError::NotFound)
    ));
    blocked(&pool, root, true).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::NotFound)
    ));
    blocked(&pool, root, false).await;
    blocked(&pool, target, true).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::NotFound)
    ));
    blocked(&pool, target, false).await;
    let invalid = sqlx::query(
        "INSERT INTO memory_sources(individual_id,memory_id,event_id) VALUES($1,$2,$3)",
    )
    .bind(Uuid::new_v4())
    .bind(target)
    .bind(root)
    .execute(&pool)
    .await;
    assert!(invalid.is_err());
    edge(&pool, root, individual(&pool).await).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::Invalid("identity anchor deletion unsupported"))
    ));
    blocked(&pool, individual(&pool).await, true).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::NotFound)
    ));
}
#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_limit_10000_is_explicit_and_no_source_memory_rejected(pool: PgPool) {
    let store = setup(&pool).await;
    let source = event(&pool, "source").await;
    let target = memory(&pool, &[source]).await;
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT i.id,md5('p05-fanout-'||g::text)::uuid,'event' FROM individuals i CROSS JOIN generate_series(1,9998) g").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,$1,md5('p05-fanout-'||g::text)::uuid FROM individuals CROSS JOIN generate_series(1,9998) g").bind(source).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        store
            .memory_deletion_closure(target, 1)
            .await
            .unwrap()
            .object_ids
            .len(),
        10000
    );
    assert_eq!(fingerprint(&pool).await, before);
    let extra = object(&pool, "event").await;
    edge(&pool, source, extra).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::Invalid("memory deletion closure too large"))
    ));
    let orphan = memory(&pool, &[]).await;
    assert!(matches!(
        store.memory_deletion_closure(orphan, 1).await,
        Err(StoreError::Invalid(_))
    ));
}

#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_actual_affective_sources_subject_and_attitude_predecessor(pool: PgPool) {
    let store = setup(&pool).await;
    let source = event(&pool, "root").await;
    let independent = event(&pool, "safe").await;
    let target = memory(&pool, &[source]).await;
    let subject = object(&pool, "relationship").await;
    sqlx::query("INSERT INTO relationships(individual_id,subject_id,subject_key) SELECT id,$1,'affect-related' FROM individuals").bind(subject).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT id,$1,$2 FROM individuals").bind(subject).bind(source).execute(&pool).await.unwrap();
    let appraisal = object(&pool, "affective_state").await;
    sqlx::query("INSERT INTO affective_appraisals(individual_id,id,subject_id,valence,arousal,goal_relevance,expectation_gap,controllability,uncertainty,brief_basis,expires_at) SELECT id,$1,$2,0,0,0,0,0,0,'fixture',clock_timestamp()+interval '1 hour' FROM individuals").bind(appraisal).bind(subject).execute(&pool).await.unwrap();
    let previous = object(&pool, "affective_attitude").await;
    let current = object(&pool, "affective_attitude").await;
    let safe = object(&pool, "affective_attitude").await;
    sqlx::query("INSERT INTO affective_attitudes(individual_id,id,topic,stance,brief_basis,status,superseded_at) SELECT id,$1,'shared','PRIVATE-OLD','fixture','superseded',clock_timestamp() FROM individuals").bind(previous).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO affective_attitudes(individual_id,id,topic,stance,brief_basis,supersedes) SELECT id,$1,'shared','PRIVATE-NEW','fixture',$2 FROM individuals").bind(current).bind(previous).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO affective_attitudes(individual_id,id,topic,stance,brief_basis) SELECT id,$1,'independent','PRIVATE-SAFE','fixture' FROM individuals").bind(safe).execute(&pool).await.unwrap();
    for (object, event) in [
        (previous, source),
        (current, independent),
        (safe, independent),
        (appraisal, independent),
    ] {
        sqlx::query("INSERT INTO affective_sources(individual_id,object_id,event_id) SELECT id,$1,$2 FROM individuals").bind(object).bind(event).execute(&pool).await.unwrap();
    }
    let got = store.memory_deletion_closure(target, 1).await.unwrap();
    for id in [source, target, subject, appraisal, previous, current] {
        assert!(got.object_ids.contains(&id));
    }
    assert!(!got.object_ids.contains(&safe));
    assert!(!got.object_ids.contains(&independent));
}

#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_mixed_goal_origins_and_optional_schema_families_fail_closed(pool: PgPool) {
    let store = setup(&pool).await;
    let source = event(&pool, "root").await;
    let target = memory(&pool, &[source]).await;
    let goal = object(&pool, "goal").await;
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) SELECT id,$1,'mixed',jsonb_build_array('interest:'||id::text||':0',$2::uuid::text),'bounded fixture','criterion','step','stop' FROM individuals").bind(goal).bind(source).execute(&pool).await.unwrap();
    assert!(
        store
            .memory_deletion_closure(target, 1)
            .await
            .unwrap()
            .object_ids
            .contains(&goal)
    );
    sqlx::query("DROP TABLE affective_sources")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::Invalid("incomplete affective deletion schema"))
    ));
    sqlx::query("DROP TABLE affective_attitudes,affective_appraisals")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        store
            .memory_deletion_closure(target, 1)
            .await
            .unwrap()
            .object_ids
            .contains(&goal)
    );
    sqlx::query("DROP TABLE autobiography_events")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::Invalid(
            "incomplete autobiography deletion schema"
        ))
    ));
    sqlx::query("DROP TABLE autobiography_beliefs,autobiographies")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        store
            .memory_deletion_closure(target, 1)
            .await
            .unwrap()
            .object_ids
            .contains(&goal)
    );
}
#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_more_than_256_actual_event_roots_refused_without_side_effects(pool: PgPool) {
    let store = setup(&pool).await;
    let target = memory(&pool, &[]).await;
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,md5('p05-multi-root-'||n::text)::uuid,'event' FROM individuals CROSS JOIN generate_series(1,257) n").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,md5('p05-multi-root-'||n::text)::uuid,'message.received','reported_claim','owner',now(),'{}',md5('p05-multi-root-'||n::text)::uuid FROM individuals CROSS JOIN generate_series(1,257) n").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,md5('p05-multi-root-'||n::text)::uuid FROM individuals CROSS JOIN generate_series(1,257) n").bind(target).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::Invalid("unsupported memory deletion sources"))
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "P05 scoped QA PostgreSQL; whole parent heavy lock required"]
async fn closure_missing_explicit_ancestor_edges_still_makes_target_unavailable(pool: PgPool) {
    let store = setup(&pool).await;
    let previous_source = event(&pool, "old private").await;
    let current_source = event(&pool, "new source").await;
    let previous = memory(&pool, &[previous_source]).await;
    let target = memory(&pool, &[current_source]).await;
    sqlx::query("UPDATE memories SET supersedes=$2 WHERE id=$1")
        .bind(target)
        .bind(previous)
        .execute(&pool)
        .await
        .unwrap();
    blocked(&pool, previous, true).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::NotFound)
    ));
    blocked(&pool, previous, false).await;
    blocked(&pool, previous_source, true).await;
    assert!(matches!(
        store.memory_deletion_closure(target, 1).await,
        Err(StoreError::NotFound)
    ));
}
