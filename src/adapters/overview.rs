//! Overview aggregates use the same public provenance rules in one read snapshot.
use super::{
    api::{VISIBLE, existing},
    console::safe_failure,
    store::{Store, StoreError},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

impl Store {
    pub async fn public_overview(&self) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let id = existing(&mut tx).await?;
        let (version, epoch, observed): (i64, i64, DateTime<Utc>) = sqlx::query_as(
            "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        // Match /memories and /goals source filters; existing() also guards the identity.
        let memories: Value = sqlx::query_scalar(&format!(
            r#"{VISIBLE}
            SELECT jsonb_build_object('total',count(*),
                'active',count(*) FILTER(WHERE m.validity='active'),
                'disputed',count(*) FILTER(WHERE m.validity='disputed'),
                'superseded',count(*) FILTER(WHERE m.validity='superseded'),
                'stale',count(*) FILTER(WHERE m.validity='stale'))
            FROM memories m WHERE m.individual_id=$1 AND NOT EXISTS(
                SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(m.individual_id,m.id))"#
        ))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let goals: Value = sqlx::query_scalar(&format!(
            r#"{VISIBLE}
            SELECT jsonb_build_object('total',count(*),
                'proposed',count(*) FILTER(WHERE g.status='proposed'),
                'active',count(*) FILTER(WHERE g.status='active'),
                'waiting',count(*) FILTER(WHERE g.status='waiting'),
                'completed',count(*) FILTER(WHERE g.status='completed'),
                'abandoned',count(*) FILTER(WHERE g.status='abandoned'))
            FROM goals g WHERE g.individual_id=$1 AND NOT EXISTS(
                SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(g.individual_id,g.id))"#
        ))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        // Same run/trigger/identity filter as /runs. Reused verbatim for all run views.
        let visible_runs = format!(
            r#"{VISIBLE}, visible_runs AS (
            SELECT r.* FROM runs r WHERE r.individual_id=$1 AND NOT EXISTS(
                SELECT 1 FROM hidden h WHERE h.individual_id=r.individual_id
                AND h.id IN (r.individual_id,r.id,r.trigger_event_id)))"#
        );
        let runs: Value = sqlx::query_scalar(&format!(
            r#"{visible_runs}
            SELECT jsonb_build_object('total',count(*),
                'queued',count(*) FILTER(WHERE status='queued'),
                'running',count(*) FILTER(WHERE status='running'),
                'committed',count(*) FILTER(WHERE status='committed'),
                'failed',count(*) FILTER(WHERE status='failed'),
                'cancelled',count(*) FILTER(WHERE status='cancelled'),
                'stale',count(*) FILTER(WHERE status='stale')) FROM visible_runs"#
        ))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let days: Vec<Value> = sqlx::query_scalar(&format!(
            r#"{visible_runs}, days AS (
            SELECT $2::date - n AS day FROM generate_series(6,0,-1) n)
            SELECT jsonb_build_object('date',d.day,'partial',d.day=$2::date,
                'created',count(r.id),'failed',count(r.id) FILTER(WHERE r.status='failed'),
                'foreground',count(r.id) FILTER(WHERE r.mode='foreground'),
                'background',count(r.id) FILTER(WHERE r.mode='background'))
            FROM days d LEFT JOIN visible_runs r
                ON r.created_at >= (d.day::timestamp AT TIME ZONE 'UTC')
                AND r.created_at < ((d.day+1)::timestamp AT TIME ZONE 'UTC')
            GROUP BY d.day ORDER BY d.day"#
        ))
        .bind(id)
        .bind(observed.date_naive())
        .fetch_all(&mut *tx)
        .await?;
        let mut failures: Vec<Value> = sqlx::query_scalar(&format!(
            r#"{visible_runs}
            SELECT jsonb_build_object('id',id,'created_at',created_at,'finished_at',finished_at,
                'mode',mode,'error_code',error_code) FROM visible_runs WHERE status='failed'
            ORDER BY created_at DESC,id DESC LIMIT 5"#
        ))
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for run in &mut failures {
            run["error_code"] = json!(safe_failure(run["error_code"].as_str()));
        }
        tx.commit().await?;
        Ok(
            json!({"individual_id":id,"state_version":version,"privacy_epoch":epoch,
            "observed_at":observed,"scope":"full_visible_metadata",
            "totals":{"memories":memories,"goals":goals,"runs":runs},
            "trend":{"timezone":"UTC","group_by":"created_at","days":days},
            "recent_failed_runs":failures}),
        )
    }
}
