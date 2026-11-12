//! Event-driven opportunity assessment. It queues cognition, never a goal.
use super::{goals::unavailable, store::*};
use crate::domain::Lifecycle;
use uuid::Uuid;
impl Store {
    pub async fn tick_discovery(&self) -> Result<Option<Uuid>, StoreError> {
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if current.lifecycle != Lifecycle::Active
            || current.runtime_config["autonomy"]["enabled"] != true
            || unavailable(&mut tx, current.id, &[current.id]).await?
        {
            return Ok(None);
        }
        let eligible: bool = sqlx::query_scalar(r#"SELECT NOT autonomy_paused AND NOT budget_frozen
 AND NOT EXISTS(SELECT 1 FROM runs WHERE individual_id=i.id AND status IN ('queued','running'))
 AND NOT EXISTS(SELECT 1 FROM goals WHERE individual_id=i.id AND status IN ('proposed','active','waiting'))
 AND NOT EXISTS(SELECT 1 FROM discovery_occurrences d WHERE d.individual_id=i.id
   AND d.created_at>clock_timestamp()-make_interval(mins=>(i.runtime_config->'autonomy'->>'idle_check_minutes')::integer))
 FROM individuals i WHERE id=$1"#).bind(current.id).fetch_one(&mut *tx).await?;
        if !eligible {
            return Ok(None);
        }
        let candidates: Vec<Uuid> = sqlx::query_scalar(r#"WITH RECURSIVE denied(id) AS (
 SELECT id FROM objects WHERE individual_id=$1 AND blocked UNION
 SELECT d.target_id FROM derivation_edges d JOIN denied ON denied.id=d.source_id WHERE d.individual_id=$1)
 SELECT e.id FROM events e JOIN documents d ON (d.individual_id,d.event_id)=(e.individual_id,e.id)
 WHERE e.individual_id=$1 AND e.kind='document.imported'
 AND NOT EXISTS(SELECT 1 FROM denied WHERE denied.id=e.id)
 AND NOT EXISTS(SELECT 1 FROM actions a JOIN events observed ON (observed.individual_id,observed.id)=(a.individual_id,a.result_event_id) WHERE a.individual_id=e.individual_id AND a.status='succeeded' AND a.capability_id IN ('documents.read','web.read_allowed') AND observed.payload->'output'->>'document_id'=e.id::text)
 AND NOT EXISTS(SELECT 1 FROM discovery_occurrences c JOIN runs attempted ON (attempted.individual_id,attempted.id)=(c.individual_id,c.run_id) WHERE c.individual_id=e.individual_id AND c.source_event_id=e.id AND NOT (attempted.model_calls=0 AND attempted.status IN ('failed','cancelled','stale')))
 ORDER BY e.sequence LIMIT 1"#).bind(current.id).fetch_all(&mut *tx).await?;
        let Some(source) = candidates.first().copied() else {
            return Ok(None);
        };
        let run = Uuid::new_v4();
        let version = current.state_version + 1;
        object(&mut tx, current.id, run, "run").await?;
        edge(&mut tx, current.id, source, run).await?;
        sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,input_manifest) VALUES($1,$2,$3,'background',$4,$5,$6,$7)")
            .bind(current.id).bind(run).bind(source).bind(version).bind(current.privacy_epoch).bind(current.config_version).bind(vec![current.id,source,run]).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO discovery_occurrences(individual_id,source_event_id,run_id) VALUES($1,$2,$3) ON CONFLICT(individual_id,source_event_id) DO UPDATE SET run_id=EXCLUDED.run_id,created_at=clock_timestamp()")
            .bind(current.id).bind(source).bind(run).execute(&mut *tx).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "operation.updated", run, version).await?;
        tx.commit().await?;
        Ok(Some(run))
    }
}
