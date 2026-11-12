//! Startup deletion barrier and bounded, persistent erasure maintenance.
use super::Runtime;
use crate::adapters::{deletion_journal::DeletionJournal, store::StoreError};
use std::{path::Path, sync::Arc, time::Duration};

impl Runtime {
    /// Call before any listener, model, indexing or ordinary state restoration.
    /// An existing individual must never acquire a silently recreated journal.
    pub async fn configure_deletion_journal(
        &mut self,
        path: Option<&Path>,
    ) -> Result<(), StoreError> {
        self.store.fence_privacy();
        self.deletion_journal = None;
        let Some(path) = path else {
            let jobs: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM privacy_operations)")
                .fetch_one(&self.database)
                .await?;
            if jobs {
                return Err(StoreError::PrivacyUnavailable);
            }
            self.store.set_privacy_ready(true);
            return Ok(());
        };
        if path.as_os_str().is_empty() {
            return Err(StoreError::PrivacyUnavailable);
        }
        let existing: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM individuals)")
            .fetch_one(&self.database)
            .await?;
        let path = path.to_owned();
        let journal = tokio::task::spawn_blocking(move || {
            let result = if existing {
                DeletionJournal::open_existing(&path)
            } else {
                DeletionJournal::open(&path)
            };
            result.map_err(|_| StoreError::PrivacyUnavailable)
        })
        .await
        .map_err(|_| StoreError::PrivacyUnavailable)??;
        let journal = Arc::new(journal);
        self.store.replay_deletion_journal(journal.clone()).await?;
        self.deletion_journal = Some(journal);
        Ok(())
    }

    pub fn deletion_journal(&self) -> Option<Arc<DeletionJournal>> {
        self.deletion_journal.clone()
    }

    pub fn deletion_ready(&self) -> bool {
        self.deletion_journal.is_some() && self.store.privacy_ready()
    }

    pub fn wake_privacy(&self) {
        self.privacy_wake.notify_one();
    }

    /// One maintenance batch. No model calls; ordinary autonomy pause does not
    /// prevent a requested erasure. Uncertain acceptance is reconciled first.
    pub async fn privacy_maintenance_once(&self) -> Result<u32, StoreError> {
        let Some(journal) = self.deletion_journal() else {
            return Ok(0);
        };
        if !self.store.privacy_ready() {
            self.store.replay_deletion_journal(journal).await?;
        }
        match self.store.purge_privacy_operations(4).await {
            Err(StoreError::NotInitialized) => Ok(0),
            result => result,
        }
    }

    pub async fn serve_privacy(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        if self.deletion_journal.is_none() {
            return;
        }
        loop {
            if *shutdown.borrow() {
                return;
            }
            match self.privacy_maintenance_once().await {
                Ok(completed) if completed > 0 => self.wake.notify_one(),
                Ok(_) => {}
                // Never log the journal, source content, database error or path.
                Err(_) => tracing::warn!("privacy maintenance incomplete; retry scheduled"),
            }
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return; }
                },
                _ = self.privacy_wake.notified() => {},
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
            }
        }
    }
}
