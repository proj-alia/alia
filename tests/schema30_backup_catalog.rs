//! Canonical schema catalog measured on the same real SQLx migrations as backup.
use serde_json::Value;
use sha2::{Digest, Sha256};
#[sqlx::test]
#[ignore = "P05 isolated PG17 catalog calibration; no models"]
async fn schema30_backup_catalog_fingerprint(pool: sqlx::PgPool) {
    sqlx::query("SET search_path=pg_catalog,public")
        .execute(&pool)
        .await
        .unwrap();
    let catalog: Value = sqlx::query_scalar(include_str!("support/p05_backup_catalog.sql"))
        .fetch_one(&pool)
        .await
        .unwrap();
    println!(
        "P05_SCHEMA30_CATALOG={:x}",
        Sha256::digest(serde_json::to_vec(&catalog).unwrap())
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT max(version) FROM public._sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        30
    );
}
