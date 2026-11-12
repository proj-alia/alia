//! Infrastructure adapters. SQL connections are bounded and errors never expose credentials.
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;
pub mod affect;
pub(crate) mod api;
pub mod autobiography;
pub mod beliefs;
mod beliefs_view;
mod capabilities;
mod completion;
pub(crate) mod console;
mod contact;
mod context;
mod decisions;
pub mod deletion;
pub mod deletion_journal;
pub mod development;
mod development_context;
pub(crate) mod development_measurements;
pub(crate) mod development_trial_context;
mod discovery;
mod documents_view;
#[cfg(feature = "local-embeddings")]
pub mod embedding;
mod entities;
pub mod export;
pub(crate) mod goal_commands;
mod goals;
mod grounding;
mod indexing;
pub mod interests;
pub mod memory_commands;
mod notes_view;
pub use indexing::{IndexFailure, IndexLease};
pub mod model;
pub mod notifications;
mod overview;
mod privacy_development;
mod privacy_operations;
pub mod relationship_revisions;
mod relationships_view;
mod retrieval;
mod reviews;
mod scheduling;
pub mod semantic;
mod social;
mod source_events;
pub mod store;
pub mod tokenizer;
pub mod web;
pub(crate) mod web_actions;
mod worker;

pub async fn connect_database(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
}

pub async fn database_ready(pool: &PgPool) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(pool)
        )
        .await,
        Ok(Ok(1))
    )
}
mod reply_wire;
