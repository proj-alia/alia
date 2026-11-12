//! Real PG personal portability; synthetic records, no production file/network/model export.
use alia::{
    adapters::{
        deletion_journal::DeletionJournal,
        store::{Store, StoreError},
    },
    domain::{
        config::{PersonaSeed, RuntimeRules},
        export::{
            EXPORT_FORMAT, EXPORT_MAX_BYTES, EXPORT_MAX_TABLE_ROWS, ExportError, IndividualExport,
            validate_json_document,
        },
    },
    runtime::capability::IMPLEMENTED_CAPABILITIES,
};
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use std::{collections::BTreeMap, fs, os::unix::fs::DirBuilderExt, path::PathBuf, sync::Arc};
use uuid::Uuid;
struct Journal {
    dir: PathBuf,
    journal: Arc<DeletionJournal>,
}
impl Journal {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("alia-p05-export-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let journal = Arc::new(DeletionJournal::open(&dir.join("deletions.jsonl")).unwrap());
        Self { dir, journal }
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "export-init",
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
async fn owner(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM individuals WHERE singleton")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn object(pool: &PgPool, kind: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(id).bind(kind).execute(pool).await.unwrap();
    id
}
async fn event(pool: &PgPool, text: &str) -> Uuid {
    let id = object(pool, "event").await;
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id,idempotency_key) SELECT id,$1,'message.received','reported_claim','owner',clock_timestamp(),jsonb_build_object('text',$2::text),$1,'RAW-COMMAND-KEY' FROM individuals WHERE singleton").bind(id).bind(text).execute(pool).await.unwrap();
    id
}
async fn memory(pool: &PgPool, source: Uuid, text: &str) -> Uuid {
    let id = object(pool, "memory").await;
    sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,$1,'semantic',$2,'general' FROM individuals WHERE singleton").bind(id).bind(text).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(id).bind(source).execute(pool).await.unwrap();
    id
}
async fn fingerprint(pool: &PgPool) -> BTreeMap<String, String> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname=current_schema() ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut hashes = BTreeMap::new();
    for table in tables {
        assert!(
            table
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        );
        let hash:String=sqlx::query_scalar(&format!("SELECT md5(coalesce(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM {table} t")).fetch_one(pool).await.unwrap();
        hashes.insert(table, hash);
    }
    hashes
}
fn empty_archive() -> IndividualExport {
    IndividualExport {
        format: EXPORT_FORMAT.into(),
        individual_id: Uuid::new_v4(),
        state_version: 1,
        privacy_epoch: 0,
        observed_at: Utc::now(),
        schema_fingerprint: "a".repeat(64),
        tables: BTreeMap::new(),
        omissions: vec![],
    }
}
#[test]
fn export_json_actual_serializer_duplicate_fields_utf8_and_limits() {
    let mut archive = empty_archive();
    archive.tables.insert(
        "events".into(),
        vec![json!({"text":"甲🙂e\u{301}\n<script>text only</script>"})],
    );
    let encoded = archive.to_json_bytes().unwrap();
    validate_json_document(&encoded).unwrap();
    assert!(std::str::from_utf8(&encoded).unwrap().contains("甲🙂"));
    for invalid in [
        br#"{"format":1,"format":2}"#.as_slice(),
        br#"{"tables":{"x":[],"x":[]}}"#.as_slice(),
        br#"{"x":[{"text":"a","text":"b"}]}"#.as_slice(),
        b"{} trailing".as_slice(),
    ] {
        assert_eq!(validate_json_document(invalid), Err(ExportError::Invalid));
    }
    archive
        .tables
        .insert("events".into(), vec![json!({}); EXPORT_MAX_TABLE_ROWS + 1]);
    assert_eq!(archive.to_json_bytes(), Err(ExportError::TooManyRows));
    archive.tables.insert(
        "events".into(),
        vec![json!({"text":"x".repeat(EXPORT_MAX_BYTES)})],
    );
    assert_eq!(archive.to_json_bytes(), Err(ExportError::TooLarge));
    archive.tables.clear();
    archive.individual_id = Uuid::nil();
    assert_eq!(archive.to_json_bytes(), Err(ExportError::Invalid));
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_complete_history_sources_stable_order_restart_and_no_mutation(pool: PgPool) {
    let s = setup(&pool).await;
    let mut events = vec![];
    for n in 0..55 {
        events.push(event(&pool, &format!("HISTORY-{n:02}🙂")).await);
    }
    let first = memory(&pool, events[0], "old interpretation").await;
    let second = memory(&pool, events[54], "new interpretation").await;
    sqlx::query("UPDATE memories SET validity='superseded' WHERE id=$1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET supersedes=$2,version=2 WHERE id=$1")
        .bind(second)
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    let old = object(&pool, "belief").await;
    let new = object(&pool, "belief").await;
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis,status,superseded_at) SELECT id,$1,'old stance','general','held','old basis','superseded',clock_timestamp() FROM individuals WHERE singleton").bind(old).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis,supersedes,version) SELECT id,$1,'new stance','general','questioned','new basis',$2,2 FROM individuals WHERE singleton").bind(new).bind(old).execute(&pool).await.unwrap();
    for (belief, e, role) in [(old, events[0], "support"), (new, events[54], "counter")] {
        sqlx::query("INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) SELECT id,$1,$2,$3 FROM individuals WHERE singleton").bind(belief).bind(e).bind(role).execute(&pool).await.unwrap();
    }
    let history = object(&pool, "autobiography").await;
    let current = object(&pool, "autobiography").await;
    sqlx::query("INSERT INTO autobiographies(individual_id,id,topic,title,interpretation,status,superseded_at) SELECT id,$1,'topic','old title','old story','superseded',clock_timestamp() FROM individuals WHERE singleton").bind(history).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO autobiographies(individual_id,id,topic,title,interpretation,supersedes,version) SELECT id,$1,'topic','new title','new story',$2,2 FROM individuals WHERE singleton").bind(current).bind(history).execute(&pool).await.unwrap();
    for a in [history, current] {
        sqlx::query("INSERT INTO autobiography_events(individual_id,autobiography_id,event_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(a).bind(events[0]).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO autobiography_beliefs(individual_id,autobiography_id,belief_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(a).bind(new).execute(&pool).await.unwrap();
    }
    let retired = object(&pool, "interest").await;
    let logical = format!("interest:{}", Uuid::new_v4());
    sqlx::query("INSERT INTO interests(individual_id,id,object_id,description,status,origin_kind) SELECT id,$1,$2,'old interest remains history','retired','experience' FROM individuals WHERE singleton").bind(&logical).bind(retired).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO interest_sources(individual_id,interest_id,event_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(&logical).bind(events[0]).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    let first_export = s.export_individual().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
    let schema_version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(matches!(schema_version, 29 | 30));
    assert_eq!(
        first_export.tables.len(),
        if schema_version == 30 { 61 } else { 56 }
    );
    assert_eq!(first_export.tables["events"].len(), 56);
    assert_eq!(first_export.tables["memories"].len(), 2);
    assert_eq!(first_export.tables["beliefs"].len(), 2);
    assert_eq!(first_export.tables["autobiographies"].len(), 2);
    let seed: serde_json::Value =
        serde_json::from_slice(include_bytes!("../config/persona.example.json")).unwrap();
    let configured = seed["personality"]["initial_interests"].as_array().unwrap();
    assert_eq!(
        first_export.tables["interest_sources"].len(),
        configured.len() + 1
    );
    assert_eq!(first_export.tables["interests"].len(), configured.len() + 1);
    let retired_export = first_export.tables["interests"]
        .iter()
        .find(|row| row["id"] == logical)
        .unwrap();
    assert_eq!(retired_export["status"], "retired");
    for description in configured {
        assert!(
            first_export.tables["interests"]
                .iter()
                .any(|row| row["description"] == *description)
        );
    }
    assert!(
        first_export.tables["memories"]
            .iter()
            .any(|r| r["validity"] == "superseded")
    );
    assert!(
        first_export.tables["belief_evidence"]
            .iter()
            .any(|r| r["role"] == "counter")
    );
    assert_eq!(first_export.individual_id, owner(&pool).await);
    let ids: Vec<_> = first_export.tables["events"]
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
    let restart = Store::new(pool.clone()).export_individual().await.unwrap();
    assert_eq!(first_export.tables, restart.tables);
    assert_eq!(first_export.omissions, restart.omissions);
    assert_eq!(first_export.schema_fingerprint, restart.schema_fingerprint);
    assert_eq!(fingerprint(&pool).await, before);
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_credentials_raw_keys_runtime_addresses_errors_and_billing_policy(pool: PgPool) {
    let s = setup(&pool).await;
    let source = event(&pool, "Personal text discusses token and error as words.").await;
    let target = memory(&pool, source, "plain human history").await;
    sqlx::query("UPDATE individuals SET runtime_config=runtime_config||jsonb_build_object('api_key','CREDENTIAL-SENTINEL','endpoint','https://deployment.invalid/private','nested',jsonb_build_object('Authorization','AUTH-SENTINEL'))").execute(&pool).await.unwrap();
    sqlx::query("UPDATE events SET payload=payload||jsonb_build_object('nested',jsonb_build_object('api-key','API-SENTINEL','raw_error','PROVIDER-RAW-SENTINEL','url','https://source.invalid/article'),'password','PASSWORD-SENTINEL') WHERE id=$1").bind(source).execute(&pool).await.unwrap();
    let run = object(&pool, "run").await;
    let lease_owner = Uuid::new_v4();
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,lease_owner,error_code,input_manifest) SELECT id,$1,$2,'foreground',state_version,privacy_epoch,config_version,$3,'PROVIDER-RAW-SENTINEL',$4 FROM individuals WHERE singleton").bind(run).bind(source).bind(lease_owner).bind(vec![target]).execute(&pool).await.unwrap();
    let window = Uuid::new_v4();
    let reservation = Uuid::new_v4();
    sqlx::query("INSERT INTO budget_windows(id,individual_id,timezone,starts_at,ends_at,charged_total) SELECT $1,id,'UTC',clock_timestamp()-interval '1 day',clock_timestamp()+interval '1 day',44 FROM individuals WHERE singleton").bind(window).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO usage_reservations(id,individual_id,run_id,lease_generation,request_key,request_digest,window_id,mode,input_bound,output_cap,reserved_tokens,status,charged_tokens,usage_unknown) SELECT $1,id,$2,0,$3,'REQUEST-SECRET-SENTINEL',$4,'foreground',40,4,44,'settled',44,true FROM individuals WHERE singleton").bind(reservation).bind(run).bind(Uuid::new_v4()).bind(window).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO idempotency_records(actor_id,route,key,body_digest,response) VALUES('owner','p05','RAW-REPLAY-KEY',repeat('f',64),jsonb_build_object('memory_id',$1::uuid))").bind(target).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    let archive = s.export_individual().await.unwrap();
    let bytes = archive.to_json_bytes().unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    for forbidden in [
        "CREDENTIAL-SENTINEL",
        "AUTH-SENTINEL",
        "API-SENTINEL",
        "PASSWORD-SENTINEL",
        "PROVIDER-RAW-SENTINEL",
        "RAW-COMMAND-KEY",
        "RAW-REPLAY-KEY",
        "REQUEST-SECRET-SENTINEL",
        &"f".repeat(64),
        "deployment.invalid",
        &lease_owner.to_string(),
    ] {
        assert!(
            !text.contains(forbidden),
            "excluded operational data returned"
        );
    }
    assert!(text.contains("Personal text discusses token and error as words."));
    assert!(text.contains("https://source.invalid/article"));
    assert_eq!(
        archive.tables["usage_reservations"][0]["charged_tokens"],
        44
    );
    assert_eq!(
        archive.tables["usage_reservations"][0]["usage_unknown"],
        true
    );
    assert!(archive.tables["usage_reservations"][0]["actual_total"].is_null());
    assert!(archive.tables["idempotency_records"].is_empty());
    assert!(
        archive
            .omissions
            .iter()
            .any(|o| o.table == "idempotency_records" && o.columns == ["*"])
    );
    assert_eq!(fingerprint(&pool).await, before);
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_pending_privacy_refused_and_completed_private_body_links_absent_billing_retained(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let journal = Journal::new();
    let source = event(&pool, "DELETED-PRIVATE-MARKER").await;
    let target = memory(&pool, source, "DELETED-PRIVATE-MARKER").await;
    let independent = event(&pool, "INDEPENDENT-SAFE-MARKER").await;
    let safe = memory(&pool, independent, "INDEPENDENT-SAFE-MARKER").await;
    let receipt = s
        .begin_memory_deletion("EXPORT-PRIVATE-KEY", target, 1, journal.journal.clone())
        .await
        .unwrap();
    assert!(matches!(
        s.export_individual().await,
        Err(StoreError::PrivacyUnavailable)
    ));
    s.purge_privacy_operations(1).await.unwrap();
    let before = fingerprint(&pool).await;
    let export = s.export_individual().await.unwrap();
    let data = String::from_utf8(export.to_json_bytes().unwrap()).unwrap();
    assert!(!data.contains("DELETED-PRIVATE-MARKER"));
    assert!(!data.contains("EXPORT-PRIVATE-KEY"));
    assert!(!data.contains(&target.to_string()));
    assert!(!data.contains(&source.to_string()));
    assert!(data.contains("INDEPENDENT-SAFE-MARKER"));
    assert!(
        export.tables["memories"]
            .iter()
            .any(|r| r["id"] == safe.to_string())
    );
    assert_eq!(
        export.tables["privacy_operations"][0]["id"],
        receipt.operation_id.to_string()
    );
    assert!(
        export.tables["privacy_operations"][0]
            .get("target_id")
            .is_none()
    );
    assert_eq!(fingerprint(&pool).await, before);
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_blocked_counter_history_relation_interest_missing_edges_no_private_descendants(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = event(&pool, "BLOCKED-SOURCE-PRIVATE").await;
    let target = memory(&pool, source, "DERIVED-MEMORY-PRIVATE").await;
    let independent = event(&pool, "INDEPENDENT-SAFE").await;
    let safe = memory(&pool, independent, "INDEPENDENT-SAFE").await;
    let belief = object(&pool, "belief").await;
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis) SELECT id,$1,'DERIVED-STANCE-PRIVATE','general','held','source' FROM individuals WHERE singleton").bind(belief).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) SELECT id,$1,$2,'counter' FROM individuals WHERE singleton").bind(belief).bind(source).execute(&pool).await.unwrap();
    let story = object(&pool, "autobiography").await;
    sqlx::query("INSERT INTO autobiographies(individual_id,id,topic,title,interpretation) SELECT id,$1,'private','DERIVED-STORY-PRIVATE','DERIVED-STORY-PRIVATE' FROM individuals WHERE singleton").bind(story).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO autobiography_beliefs(individual_id,autobiography_id,belief_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(story).bind(belief).execute(&pool).await.unwrap();
    let relation = object(&pool, "relationship").await;
    sqlx::query("INSERT INTO relationships(individual_id,subject_id,subject_key,expectations) SELECT id,$1,'PRIVATE-RELATION','[\"DERIVED-RELATION-PRIVATE\"]' FROM individuals WHERE singleton").bind(relation).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(relation).bind(source).execute(&pool).await.unwrap();
    let interest = object(&pool, "interest").await;
    let logical = format!("interest:{}", Uuid::new_v4());
    sqlx::query("INSERT INTO interests(individual_id,id,object_id,description,origin_kind) SELECT id,$1,$2,'DERIVED-INTEREST-PRIVATE','experience' FROM individuals WHERE singleton").bind(&logical).bind(interest).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO interest_sources(individual_id,interest_id,event_id) SELECT id,$1,$2 FROM individuals WHERE singleton").bind(&logical).bind(source).execute(&pool).await.unwrap();
    let goal = object(&pool, "goal").await;
    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition) SELECT id,$1,'interest',jsonb_build_array($2::text),'DERIVED-GOAL-PRIVATE','criterion','step','stop' FROM individuals WHERE singleton").bind(goal).bind(&logical).execute(&pool).await.unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    let data = String::from_utf8(
        s.export_individual()
            .await
            .unwrap()
            .to_json_bytes()
            .unwrap(),
    )
    .unwrap();
    for forbidden in [
        "BLOCKED-SOURCE-PRIVATE",
        "DERIVED-MEMORY-PRIVATE",
        "DERIVED-STANCE-PRIVATE",
        "DERIVED-STORY-PRIVATE",
        "DERIVED-RELATION-PRIVATE",
        "DERIVED-INTEREST-PRIVATE",
        "DERIVED-GOAL-PRIVATE",
    ] {
        assert!(!data.contains(forbidden));
    }
    for id in [source, target, belief, story, relation, interest, goal] {
        assert!(!data.contains(&id.to_string()));
    }
    assert!(data.contains(&safe.to_string()));
    assert!(data.contains("INDEPENDENT-SAFE"));
}

async fn wait_for_contact_lock(pool: &PgPool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='contact_windows'::regclass AND mode='AccessShareLock' AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool).await.unwrap();
        if waiting {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "real export query did not reach lock"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_concurrent_real_deletion_different_store_generation_fails_no_late_archive(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let journal = Journal::new();
    let source = event(&pool, "RACE-PRIVATE").await;
    let target = memory(&pool, source, "RACE-PRIVATE").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE contact_windows IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let exporter = s.clone();
    let task = tokio::spawn(async move { exporter.export_individual().await });
    wait_for_contact_lock(&pool).await;
    // Independent Store emulates a separate service/process: no shared AtomicBool.
    let other = Store::new(pool.clone());
    other
        .begin_memory_deletion("race-delete", target, 1, journal.journal.clone())
        .await
        .unwrap();
    other.purge_privacy_operations(1).await.unwrap();
    assert!(s.privacy_ready());
    blocker.rollback().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(
            result,
            Err(StoreError::StaleState) | Err(StoreError::PrivacyUnavailable)
        ),
        "old personal archive escaped after actual deletion"
    );
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_concurrent_billing_write_without_state_version_change_still_fails(pool: PgPool) {
    let s = setup(&pool).await;
    let window = Uuid::new_v4();
    sqlx::query("INSERT INTO budget_windows(id,individual_id,timezone,starts_at,ends_at,charged_total) SELECT $1,id,'UTC',clock_timestamp()-interval '1 day',clock_timestamp()+interval '1 day',10 FROM individuals WHERE singleton").bind(window).execute(&pool).await.unwrap();
    let before = s.snapshot().await.unwrap().unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE contact_windows IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let exporter = s.clone();
    let task = tokio::spawn(async move { exporter.export_individual().await });
    wait_for_contact_lock(&pool).await;
    sqlx::query("UPDATE budget_windows SET charged_total=charged_total+7 WHERE id=$1")
        .bind(window)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().state_version,
        before.state_version
    );
    blocker.rollback().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(StoreError::StaleState)),
        "archive returned while stored billing changed during capture"
    );
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_unknown_column_constraint_or_table_explicitly_refused(pool: PgPool) {
    let s = setup(&pool).await;
    event(&pool, "must never return partial body").await;
    sqlx::query("ALTER TABLE memories ADD COLUMN future_private text")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.export_individual().await,
        Err(StoreError::Invalid("personal export schema unsupported"))
    ));
    sqlx::query("ALTER TABLE memories DROP COLUMN future_private")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE memories ADD CONSTRAINT p05_unregistered CHECK(version>0)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.export_individual().await,
        Err(StoreError::Invalid("personal export schema unsupported"))
    ));
    sqlx::query("ALTER TABLE memories DROP CONSTRAINT p05_unregistered")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE _unregistered_store(id uuid)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.export_individual().await,
        Err(StoreError::Invalid("personal export schema unsupported"))
    ));
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_actual_pg_large_body_refuses_64mib_without_partial_result(pool: PgPool) {
    let s = setup(&pool).await;
    let source = event(&pool, "initial").await;
    sqlx::query(
        "UPDATE events SET payload=jsonb_build_object('text',repeat('x',67108864)) WHERE id=$1",
    )
    .bind(source)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        s.export_individual().await,
        Err(StoreError::Invalid("personal export exceeds 64 MiB"))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT octet_length(payload->>'text')::bigint FROM events WHERE id=$1"
        )
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap(),
        67108864
    );
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_actual_pg_table_row_limit_refuses_without_truncation(pool: PgPool) {
    let s = setup(&pool).await;
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT id,md5('export-event-'||n::text)::uuid,'event' FROM individuals CROSS JOIN generate_series(1,100000) n WHERE singleton").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,md5('export-event-'||n::text)::uuid,'message.received','reported_claim','owner',clock_timestamp(),'{}',md5('export-event-'||n::text)::uuid FROM individuals CROSS JOIN generate_series(1,100000) n WHERE singleton").execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap(),
        100001
    );
    assert!(matches!(
        s.export_individual().await,
        Err(StoreError::Invalid(
            "personal export table exceeds 100000 rows"
        ))
    ));
}

#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_final_generation_reopened_after_recovery_rejects_old_snapshot(pool: PgPool) {
    let s = setup(&pool).await;
    event(&pool, "GENERATION-PRIVATE").await;
    for lifecycle in ["paused", "terminated", "archived"] {
        sqlx::query("UPDATE individuals SET lifecycle=$1,state_version=state_version+1")
            .bind(lifecycle)
            .execute(&pool)
            .await
            .unwrap();
        let history = s.export_individual().await.unwrap();
        assert_eq!(history.tables["individuals"][0]["lifecycle"], lifecycle);
        assert!(
            history.tables["events"]
                .iter()
                .any(|e| e["payload"]["text"] == "GENERATION-PRIVATE")
        );
    }
    let archive = s.export_individual().await.unwrap();
    for rows in archive.tables.values() {
        for row in rows {
            if let Some(id) = row.get("individual_id") {
                assert_eq!(id, archive.individual_id.to_string().as_str());
            }
        }
    }
    let journal = Journal::new();
    let before = fingerprint(&pool).await;
    let generation = s.privacy_generation();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE contact_windows IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let exporter = s.clone();
    let task = tokio::spawn(async move { exporter.export_individual().await });
    wait_for_contact_lock(&pool).await;
    s.fence_privacy();
    s.replay_deletion_journal(journal.journal.clone())
        .await
        .unwrap();
    assert!(s.privacy_ready());
    assert!(s.privacy_generation() > generation);
    blocker.rollback().await.unwrap();
    assert_eq!(fingerprint(&pool).await, before);
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(StoreError::PrivacyUnavailable)),
        "old generation archive escaped after reopening"
    );
}
#[sqlx::test]
#[ignore = "P05 owned real PG; whole parent heavy lock"]
async fn export_final_hidden_execution_retains_stored_billing_without_private_association(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let journal = Journal::new();
    let source = event(&pool, "BILLED-PRIVATE").await;
    let target = memory(&pool, source, "BILLED-PRIVATE").await;
    let run = object(&pool, "run").await;
    sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,input_manifest) SELECT id,$1,$2,'foreground',state_version,privacy_epoch,config_version,$3 FROM individuals WHERE singleton").bind(run).bind(source).bind(vec![target]).execute(&pool).await.unwrap();
    let window = Uuid::new_v4();
    let reservation = Uuid::new_v4();
    sqlx::query("INSERT INTO budget_windows(id,individual_id,timezone,starts_at,ends_at,charged_total) SELECT $1,id,'UTC',clock_timestamp()-interval '1 day',clock_timestamp()+interval '1 day',48 FROM individuals WHERE singleton").bind(window).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO usage_reservations(id,individual_id,run_id,lease_generation,request_key,request_digest,window_id,mode,input_bound,output_cap,reserved_tokens,status,charged_tokens,usage_unknown) SELECT $1,id,$2,0,$3,repeat('e',64),$4,'foreground',44,4,48,'settled',48,true FROM individuals WHERE singleton").bind(reservation).bind(run).bind(Uuid::new_v4()).bind(window).execute(&pool).await.unwrap();
    s.begin_memory_deletion("billing-delete", target, 1, journal.journal.clone())
        .await
        .unwrap();
    s.purge_privacy_operations(1).await.unwrap();
    let before = fingerprint(&pool).await;
    let archive = s.export_individual().await.unwrap();
    let row = &archive.tables["usage_reservations"][0];
    assert_eq!(row["id"], reservation.to_string());
    assert_eq!(row["charged_tokens"], 48);
    assert_eq!(row["usage_unknown"], true);
    assert!(row["actual_total"].is_null());
    for field in [
        "run_id",
        "request_key",
        "request_digest",
        "lease_generation",
    ] {
        assert!(row.get(field).is_none());
    }
    assert_eq!(archive.tables["budget_windows"][0]["charged_total"], 48);
    let bytes = String::from_utf8(archive.to_json_bytes().unwrap()).unwrap();
    assert!(!bytes.contains("BILLED-PRIVATE"));
    assert!(!bytes.contains(&run.to_string()));
    assert_eq!(fingerprint(&pool).await, before);
}
