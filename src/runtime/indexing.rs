//! Local, derived memory maintenance. Never calls a generative model or adds life events.
use crate::{
    adapters::{
        IndexFailure,
        store::{Store, StoreError},
    },
    domain::retrieval::IndexedChunk,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum IndexEncodingError {
    Invalid,
    Unavailable,
}
#[async_trait::async_trait]
pub trait MemoryEncoder: Send + Sync {
    async fn encode_memory(&self, text: String) -> Result<Vec<IndexedChunk>, IndexEncodingError>;
    async fn encode_query(&self, _text: String) -> Result<Vec<f32>, IndexEncodingError> {
        Err(IndexEncodingError::Unavailable)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum IndexOutcome {
    Idle,
    Indexed(Uuid),
    Deferred(Uuid),
    Fenced(Uuid),
}
pub struct IndexWorker {
    store: Store,
    encoder: Arc<dyn MemoryEncoder>,
    owner: Uuid,
}
impl IndexWorker {
    pub fn new(store: Store, encoder: Arc<dyn MemoryEncoder>) -> Self {
        Self {
            store,
            encoder,
            owner: Uuid::new_v4(),
        }
    }
    pub async fn run_once(&self) -> Result<IndexOutcome, StoreError> {
        let Some(lease) = self.store.claim_memory_index(self.owner).await? else {
            return Ok(IndexOutcome::Idle);
        };
        let id = lease.memory_id();
        let encoded = tokio::time::timeout(
            Duration::from_secs(60),
            self.encoder.encode_memory(lease.content().to_owned()),
        )
        .await;
        let result = match encoded {
            Ok(Ok(chunks)) => self.store.finish_memory_index(&lease, &chunks).await,
            error => {
                let failure = match error {
                    Err(_) => IndexFailure::Timeout,
                    Ok(Err(IndexEncodingError::Invalid)) => IndexFailure::Invalid,
                    _ => IndexFailure::Encoding,
                };
                return match self.store.fail_memory_index(&lease, failure).await {
                    Ok(()) => Ok(IndexOutcome::Deferred(id)),
                    Err(StoreError::StaleState | StoreError::LeaseLost | StoreError::NotFound) => {
                        Ok(IndexOutcome::Fenced(id))
                    }
                    Err(e) => Err(e),
                };
            }
        };
        match result {
            Ok(()) => Ok(IndexOutcome::Indexed(id)),
            Err(StoreError::StaleState | StoreError::LeaseLost | StoreError::NotFound) => {
                Ok(IndexOutcome::Fenced(id))
            }
            Err(StoreError::Invalid(_)) => {
                self.store
                    .fail_memory_index(&lease, IndexFailure::Invalid)
                    .await?;
                Ok(IndexOutcome::Deferred(id))
            }
            Err(e) => Err(e),
        }
    }
    pub async fn serve(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                break;
            }
            let result = tokio::select! {biased;
                _=shutdown.changed()=>break,
                result=self.run_once()=>result,
            };
            let delay = match result {
                Ok(IndexOutcome::Indexed(_) | IndexOutcome::Fenced(_)) => Duration::from_millis(25),
                Ok(_) => Duration::from_secs(10),
                Err(_) => {
                    tracing::warn!("Local index maintenance failed; retrying after backoff");
                    Duration::from_secs(30)
                }
            };
            tokio::select! {biased;_=shutdown.changed()=>break,_=tokio::time::sleep(delay)=>{}}
        }
    }
}
