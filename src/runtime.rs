use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use subtle::ConstantTimeEq;
pub mod capability;
pub mod contact;
pub(crate) mod development_trials;
pub mod execution;
pub mod indexing;
pub mod model;
pub mod privacy;
pub mod review;
pub mod scheduling;
pub mod web;
pub mod worker;

#[derive(Clone)]
pub struct Runtime {
    pub database: PgPool,
    pub store: crate::adapters::store::Store,
    pub wake: Arc<tokio::sync::Notify>,
    pub shutdown: tokio::sync::watch::Sender<bool>,
    pub event_slots: Arc<tokio::sync::Semaphore>,
    pub model_status: model::ModelStatus,
    pub indexing_enabled: bool,
    pub model_connection: model::ModelConnectionInfo,
    pub started_at: chrono::DateTime<chrono::Utc>,
    deletion_journal: Option<Arc<crate::adapters::deletion_journal::DeletionJournal>>,
    privacy_wake: Arc<tokio::sync::Notify>,
    token_digest: Arc<[u8; 32]>,
}

impl Runtime {
    /// Restore validates saved rules against the binary's actual registry, without
    /// reinitializing the current persona from its seed.
    pub async fn verify_saved_configuration(
        &self,
        implemented: &[&str],
    ) -> Result<(), RestoreError> {
        if let Some(individual) = self.store.snapshot().await? {
            crate::domain::config::PersonaSeed::validate(individual.seed)?;
            crate::domain::config::RuntimeRules::validate(individual.runtime_config, implemented)?;
        }
        Ok(())
    }

    pub fn new(database: PgPool, token: &str) -> Result<Self, ConfigError> {
        if token.len() < 32 || !token.bytes().all(|c| c.is_ascii_graphic()) {
            return Err(ConfigError::InvalidToken);
        }
        Ok(Self {
            shutdown: tokio::sync::watch::channel(false).0,
            event_slots: Arc::new(tokio::sync::Semaphore::new(16)),
            store: crate::adapters::store::Store::new(database.clone()),
            wake: Arc::new(tokio::sync::Notify::new()),
            model_status: model::ModelStatus {
                mode: model::ProviderMode::Live,
                readiness: model::ModelReadiness::Unavailable,
            },
            database,
            indexing_enabled: false,
            model_connection: Default::default(),
            started_at: chrono::Utc::now(),
            deletion_journal: None,
            privacy_wake: Arc::new(tokio::sync::Notify::new()),
            token_digest: Arc::new(Sha256::digest(token).into()),
        })
    }

    pub fn authorized(&self, token: &str) -> bool {
        let candidate: [u8; 32] = Sha256::digest(token).into();
        bool::from(self.token_digest.as_ref().ct_eq(&candidate))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("failed to read saved individual")]
    Storage(#[from] crate::adapters::store::StoreError),
    #[error("saved configuration is incompatible with this runtime")]
    Configuration(#[from] crate::domain::config::ConfigurationError),
}

// Deliberately no Debug: deployment secrets must not enter logs.
pub struct Deployment {
    pub database_url: String,
    pub instance_token: String,
    pub bind: SocketAddr,
    pub web_dir: PathBuf,
    pub deletion_journal: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("DATABASE_URL is required")]
    MissingDatabase,
    #[error(
        "ALIA_INSTANCE_TOKEN must contain at least 32 printable ASCII bytes; generate it randomly"
    )]
    InvalidToken,
    #[error("ALIA_BIND must be an IP address and port")]
    InvalidBind,
    #[error("ALIA_DELETION_JOURNAL must be a nonempty local path")]
    InvalidJournalPath,
}

impl Deployment {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("DATABASE_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .ok_or(ConfigError::MissingDatabase)?;
        let instance_token =
            std::env::var("ALIA_INSTANCE_TOKEN").map_err(|_| ConfigError::InvalidToken)?;
        let bind = std::env::var("ALIA_BIND")
            .unwrap_or_else(|_| "127.0.0.1:3000".into())
            .parse()
            .map_err(|_| ConfigError::InvalidBind)?;
        let web_dir = std::env::var("ALIA_WEB_DIR")
            .unwrap_or_else(|_| "web/dist".into())
            .into();
        let deletion_journal = match std::env::var_os("ALIA_DELETION_JOURNAL") {
            Some(path) if path.is_empty() => return Err(ConfigError::InvalidJournalPath),
            Some(path) => Some(path.into()),
            None => None,
        };
        Ok(Self {
            database_url,
            instance_token,
            bind,
            web_dir,
            deletion_journal,
        })
    }
}
