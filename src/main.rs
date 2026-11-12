use alia::runtime::{
    model::{ModelError, ModelProvider, UnavailableProvider},
    worker::Worker,
};
use alia::{
    adapters,
    runtime::{Deployment, Runtime, capability::IMPLEMENTED_CAPABILITIES},
    service,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "alia=info".into()),
        )
        .init();
    let config = Deployment::from_env()?;
    // Validate the token before opening a network connection.
    let lazy_pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy(&config.database_url)
        .map_err(|_| "invalid database configuration")?;
    Runtime::new(lazy_pool, &config.instance_token)?;
    let database = adapters::connect_database(&config.database_url)
        .await
        .map_err(|_| "database connection failed; check deployment configuration")?;
    adapters::store::Store::new(database.clone())
        .migrate()
        .await
        .map_err(|_| "database migration failed; service will not start")?;
    let mut runtime = Runtime::new(database, &config.instance_token)?;
    runtime
        .configure_deletion_journal(config.deletion_journal.as_deref())
        .await
        .map_err(|_| "deletion journal unavailable or inconsistent; service will not start")?;
    // Registration describes implementations; saved configuration separately grants access.
    runtime
        .verify_saved_configuration(IMPLEMENTED_CAPABILITIES)
        .await
        .map_err(|_| "saved configuration is incompatible with the available capabilities")?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let provider: Arc<dyn ModelProvider> = match adapters::model::ChatProvider::from_env() {
        Ok(provider) => {
            runtime.model_connection = provider.public_connection_info();
            tracing::info!(
                input_accounting = provider.input_accounting(),
                "Model adapter initialized"
            );
            Arc::new(provider)
        }
        Err(error) if error.error == ModelError::Unavailable => Arc::new(UnavailableProvider),
        Err(_) => return Err("invalid model deployment configuration".into()),
    };
    runtime.model_status = provider.status();
    let shutdown_tx = runtime.shutdown.clone();
    let shutdown_rx = shutdown_tx.subscribe();
    let index_task = start_indexing(&mut runtime).await?;
    let privacy_task = tokio::spawn(runtime.clone().serve_privacy(shutdown_tx.subscribe()));
    let worker = Worker::new(
        runtime.store.clone(),
        provider,
        Arc::new(adapters::web::HttpWebReader),
        runtime.wake.clone(),
    );
    let worker_task = tokio::spawn(worker.serve(shutdown_rx));
    tracing::info!(address = %config.bind, "Alia service listening; individual features under development");
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let signal_tx = shutdown_tx.clone();
    let shutdown = async move {
        #[cfg(unix)]
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
        let _ = signal_tx.send(true);
    };
    let result = axum::serve(listener, service::router(runtime, config.web_dir))
        .with_graceful_shutdown(shutdown)
        .await;
    let _ = shutdown_tx.send(true);
    worker_task.await.map_err(|_| "worker task failed")?;
    privacy_task
        .await
        .map_err(|_| "privacy worker task failed")?;
    if let Some(task) = index_task {
        task.await.map_err(|_| "index worker task failed")?;
    }
    result?;
    Ok(())
}

async fn start_indexing(
    runtime: &mut Runtime,
) -> Result<Option<tokio::task::JoinHandle<()>>, Box<dyn std::error::Error>> {
    let Some(directory) = std::env::var_os("ALIA_EMBEDDING_DIR") else {
        return Ok(None);
    };
    if directory.is_empty() {
        return Err("ALIA_EMBEDDING_DIR must be a nonempty local asset directory".into());
    }
    #[cfg(not(feature = "local-embeddings"))]
    {
        let _ = (runtime, directory);
        Err("local embeddings requested but binary lacks local-embeddings feature".into())
    }
    #[cfg(feature = "local-embeddings")]
    {
        let model = tokio::task::spawn_blocking(move || {
            adapters::embedding::LocalEmbedding::load(std::path::Path::new(&directory))
        })
        .await
        .map_err(|_| "local encoder initialization task failed")?
        .map_err(|_| "local embedding assets unavailable or invalid")?;
        let encoder = Arc::new(adapters::embedding::CpuMemoryEncoder::new(model));
        runtime.store = runtime.store.clone().with_memory_encoder(encoder.clone());
        let worker = alia::runtime::indexing::IndexWorker::new(runtime.store.clone(), encoder);
        runtime.indexing_enabled = true;
        Ok(Some(tokio::spawn(
            worker.serve(runtime.shutdown.subscribe()),
        )))
    }
}
