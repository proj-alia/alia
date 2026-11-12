//! Durable local-index queue. No model inference, private text logs, or state-version changes.
use super::{
    api::VISIBLE,
    goals::unavailable,
    semantic::{EmbeddingSnapshot, commit_index, encode_chunks},
    store::*,
};
use crate::domain::retrieval::{INDEX_PROFILE, IndexedChunk};
use uuid::Uuid;

pub struct IndexLease {
    snapshot: EmbeddingSnapshot,
    owner: Uuid,
    generation: i64,
}
impl IndexLease {
    pub fn content(&self) -> &str {
        self.snapshot.content()
    }
    pub fn memory_id(&self) -> Uuid {
        self.snapshot.id
    }
}
#[derive(Clone, Copy)]
pub enum IndexFailure {
    Encoding,
    Timeout,
    Invalid,
}
impl IndexFailure {
    fn code(self) -> &'static str {
        match self {
            Self::Encoding => "encoding_failed",
            Self::Timeout => "encoding_timeout",
            Self::Invalid => "invalid_encoding",
        }
    }
}
async fn require_lease(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    lease: &IndexLease,
) -> Result<(), StoreError> {
    let s = &lease.snapshot;
    let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memory_index_jobs j JOIN individuals i ON i.id=j.individual_id JOIN memories m ON (m.individual_id,m.id)=(j.individual_id,j.memory_id) WHERE j.individual_id=$1 AND j.memory_id=$2 AND j.profile=$3 AND j.status='running' AND j.lease_owner=$4 AND j.generation=$5 AND j.lease_expires_at>clock_timestamp() AND j.memory_version=$6 AND j.privacy_epoch=$7 AND j.content_digest=$8 AND i.privacy_epoch=$7 AND i.lifecycle='active' AND NOT i.autonomy_paused AND m.validity IN ('active','superseded') AND m.version=$6 AND encode(sha256(convert_to(m.content,'UTF8')),'hex')=$8)")
        .bind(s.individual).bind(s.id).bind(INDEX_PROFILE).bind(lease.owner).bind(lease.generation).bind(s.version).bind(s.epoch).bind(&s.digest).fetch_one(&mut **tx).await?;
    if !ok || unavailable(tx, s.individual, &[s.individual, s.id]).await? {
        return Err(StoreError::LeaseLost);
    }
    Ok(())
}
impl Store {
    /// Discover up to 32 changed memories and claim one. Pure computation is
    /// safely repeatable; at most three attempts per exact content/epoch/profile.
    pub async fn claim_memory_index(&self, owner: Uuid) -> Result<Option<IndexLease>, StoreError> {
        let mut tx = self.write().await?;
        let Some(current) = state(&mut tx).await? else {
            return Ok(None);
        };
        if current.lifecycle != crate::domain::Lifecycle::Active
            || sqlx::query_scalar::<_, bool>("SELECT autonomy_paused FROM individuals WHERE id=$1")
                .bind(current.id)
                .fetch_one(&mut *tx)
                .await?
            || unavailable(&mut tx, current.id, &[current.id]).await?
        {
            return Ok(None);
        }
        let foreground:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs r WHERE r.individual_id=$1 AND r.mode='foreground' AND r.status IN ('queued','running') AND r.state_version=$2 AND r.privacy_epoch=$3)").bind(current.id).bind(current.state_version).bind(current.privacy_epoch).fetch_one(&mut *tx).await?;
        if foreground {
            return Ok(None);
        }
        // Obsolete tasks are fenced before source content is read.
        sqlx::query(&format!("{VISIBLE} UPDATE memory_index_jobs j SET status='cancelled',lease_owner=NULL,lease_expires_at=NULL,generation=generation+1,error_code='source_changed',updated_at=clock_timestamp() WHERE j.individual_id=$1 AND j.profile=$2 AND j.status IN ('queued','running') AND EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=j.individual_id AND h.id IN (j.individual_id,j.memory_id))"))
            .bind(current.id).bind(INDEX_PROFILE).execute(&mut *tx).await?;
        sqlx::query("UPDATE memory_index_jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'queued' END,lease_owner=NULL,lease_expires_at=NULL,error_code='lease_expired',updated_at=clock_timestamp() WHERE individual_id=$1 AND profile=$2 AND status='running' AND lease_expires_at<=clock_timestamp()")
            .bind(current.id).bind(INDEX_PROFILE).execute(&mut *tx).await?;
        // Existing failed work is not retried forever. Changed snapshots reset the
        // attempt counter but preserve a monotonic generation fencing old workers.
        sqlx::query(&format!(r#"{VISIBLE} INSERT INTO memory_index_jobs(individual_id,memory_id,profile,memory_version,privacy_epoch,content_digest,status)
          SELECT m.individual_id,m.id,$2,m.version,$3,encode(sha256(convert_to(m.content,'UTF8')),'hex'),'queued'
          FROM memories m LEFT JOIN memory_index_jobs j ON (j.individual_id,j.memory_id,j.profile)=(m.individual_id,m.id,$2)
          WHERE m.individual_id=$1 AND m.validity IN ('active','superseded')
          AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=m.individual_id AND h.id IN (m.individual_id,m.id))
          AND NOT EXISTS(SELECT 1 FROM memory_vector_sets s WHERE (s.individual_id,s.memory_id,s.profile)=(m.individual_id,m.id,$2)
            AND s.memory_version=m.version AND s.privacy_epoch=$3 AND s.content_digest=encode(sha256(convert_to(m.content,'UTF8')),'hex')
            AND s.chunk_count=(SELECT count(*) FROM memory_vectors v WHERE (v.individual_id,v.memory_id,v.profile)=(s.individual_id,s.memory_id,s.profile)))
          AND (j.memory_id IS NULL OR j.memory_version<>m.version OR j.privacy_epoch<>$3 OR j.content_digest<>encode(sha256(convert_to(m.content,'UTF8')),'hex') OR j.status IN ('ready','cancelled'))
          ORDER BY m.created_at,m.id LIMIT 32
          ON CONFLICT(individual_id,memory_id,profile) DO UPDATE SET memory_version=EXCLUDED.memory_version,privacy_epoch=EXCLUDED.privacy_epoch,content_digest=EXCLUDED.content_digest,status='queued',attempts=0,generation=memory_index_jobs.generation+1,lease_owner=NULL,lease_expires_at=NULL,available_at=clock_timestamp(),error_code=NULL,updated_at=clock_timestamp()"#))
            .bind(current.id).bind(INDEX_PROFILE).bind(current.privacy_epoch).execute(&mut *tx).await?;
        let row:Option<(Uuid,i64,i64,String,String,i64)>=sqlx::query_as(&format!(r#"{VISIBLE} SELECT j.memory_id,j.memory_version,j.privacy_epoch,j.content_digest,m.content,j.generation
           FROM memory_index_jobs j JOIN memories m ON (m.individual_id,m.id)=(j.individual_id,j.memory_id)
           WHERE j.individual_id=$1 AND j.profile=$2 AND j.status='queued' AND j.attempts<3 AND j.available_at<=clock_timestamp()
           AND j.memory_version=m.version AND j.privacy_epoch=$3 AND j.content_digest=encode(sha256(convert_to(m.content,'UTF8')),'hex')
           AND m.validity IN ('active','superseded') AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=j.individual_id AND h.id IN (j.individual_id,j.memory_id))
           ORDER BY j.available_at,j.memory_id LIMIT 1"#)).bind(current.id).bind(INDEX_PROFILE).bind(current.privacy_epoch).fetch_optional(&mut *tx).await?;
        let Some((id, version, epoch, digest, content, generation)) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        sqlx::query("UPDATE memory_index_jobs SET status='running',attempts=attempts+1,generation=generation+1,lease_owner=$4,lease_expires_at=clock_timestamp()+interval '120 seconds',updated_at=clock_timestamp() WHERE individual_id=$1 AND memory_id=$2 AND profile=$3")
            .bind(current.id).bind(id).bind(INDEX_PROFILE).bind(owner).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(IndexLease {
            snapshot: EmbeddingSnapshot {
                individual: current.id,
                id,
                version,
                epoch,
                content,
                digest,
            },
            owner,
            generation: generation + 1,
        }))
    }
    pub async fn finish_memory_index(
        &self,
        lease: &IndexLease,
        chunks: &[IndexedChunk],
    ) -> Result<(), StoreError> {
        let encoded = encode_chunks(&lease.snapshot, chunks)?;
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        commit_index(&mut tx, &lease.snapshot, chunks, encoded).await?;
        sqlx::query("UPDATE memory_index_jobs SET status='ready',lease_owner=NULL,lease_expires_at=NULL,error_code=NULL,updated_at=clock_timestamp() WHERE individual_id=$1 AND memory_id=$2 AND profile=$3")
            .bind(lease.snapshot.individual).bind(lease.snapshot.id).bind(INDEX_PROFILE).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn fail_memory_index(
        &self,
        lease: &IndexLease,
        failure: IndexFailure,
    ) -> Result<(), StoreError> {
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        sqlx::query("UPDATE memory_index_jobs SET status=CASE WHEN attempts>=3 OR $4='invalid_encoding' THEN 'failed' ELSE 'queued' END,available_at=clock_timestamp()+interval '60 seconds',lease_owner=NULL,lease_expires_at=NULL,error_code=$4,updated_at=clock_timestamp() WHERE individual_id=$1 AND memory_id=$2 AND profile=$3")
            .bind(lease.snapshot.individual).bind(lease.snapshot.id).bind(INDEX_PROFILE).bind(failure.code()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
