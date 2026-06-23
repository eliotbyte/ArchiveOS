mod asset_reconcile_scheduler;
mod blob_gc_scheduler;
mod error;
mod inbox_watcher;
mod media;
mod preview_backfill;
mod routes;
mod subscription_scheduler;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use archiveos_core::{Registry, Vault};
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub config_dir: PathBuf,
}

impl AppState {
    fn registry(&self) -> Result<Registry, archiveos_contract::VaultError> {
        Registry::open(&self.config_dir)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let config_dir = std::env::var("ARCHIVEOS_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("config"));
    let addr: SocketAddr = std::env::var("ARCHIVEOS_LISTEN")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .expect("invalid ARCHIVEOS_LISTEN");

    if let Err(err) = bootstrap_default_vault(&config_dir) {
        tracing::warn!("default vault bootstrap skipped: {err}");
    }

    if inbox_watcher::enabled() {
        let vault_name = std::env::var("ARCHIVEOS_DEFAULT_VAULT")
            .expect("ARCHIVEOS_DEFAULT_VAULT required when inbox watch enabled");
        inbox_watcher::spawn(config_dir.clone(), vault_name.clone());
        tracing::info!("inbox watcher started");
    }

    if subscription_scheduler::enabled() {
        let vault_name = std::env::var("ARCHIVEOS_DEFAULT_VAULT")
            .expect("ARCHIVEOS_DEFAULT_VAULT required when subscription scheduler enabled");
        subscription_scheduler::spawn(config_dir.clone(), vault_name);
        tracing::info!("subscription scheduler started");
    }

    if asset_reconcile_scheduler::enabled() {
        let vault_name = std::env::var("ARCHIVEOS_DEFAULT_VAULT")
            .expect("ARCHIVEOS_DEFAULT_VAULT required when asset reconcile scheduler enabled");
        asset_reconcile_scheduler::spawn(config_dir.clone(), vault_name);
        tracing::info!("asset reconcile scheduler started");
    }

    if blob_gc_scheduler::enabled() {
        let vault_name = std::env::var("ARCHIVEOS_DEFAULT_VAULT")
            .expect("ARCHIVEOS_DEFAULT_VAULT required when blob gc scheduler enabled");
        blob_gc_scheduler::spawn(config_dir.clone(), vault_name);
        tracing::info!("blob gc scheduler started");
    }

    if preview_backfill::enabled() {
        let vault_name = std::env::var("ARCHIVEOS_DEFAULT_VAULT")
            .expect("ARCHIVEOS_DEFAULT_VAULT required when preview backfill on start enabled");
        preview_backfill::spawn(config_dir.clone(), vault_name);
        tracing::info!("preview backfill on start queued");
    }

    let state = Arc::new(AppState { config_dir });
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let mut app = Router::new()
        .merge(routes::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    if let Some(web_dir) = web_static_dir() {
        tracing::info!("serving web UI from {}", web_dir.display());
        let index = web_dir.join("index.html");
        app = app.fallback_service(
            ServeDir::new(web_dir).not_found_service(ServeFile::new(index)),
        );
    } else {
        tracing::info!("web UI static dir not found; API only");
    }

    let listener = TcpListener::bind(addr).await.expect("bind failed");
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await.expect("server failed");
}

fn web_static_dir() -> Option<PathBuf> {
    let dir = std::env::var("ARCHIVEOS_WEB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/usr/share/archiveos/web"));
    if dir.join("index.html").is_file() {
        Some(dir)
    } else {
        None
    }
}

fn bootstrap_default_vault(config_dir: &PathBuf) -> Result<(), archiveos_contract::VaultError> {
    let name = match std::env::var("ARCHIVEOS_DEFAULT_VAULT") {
        Ok(name) if !name.is_empty() => name,
        _ => return Ok(()),
    };
    let path = std::env::var("ARCHIVEOS_DEFAULT_VAULT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/vaults/default"));

    let registry = Registry::open(config_dir)?;
    if registry.get(&name).is_ok() {
        return Ok(());
    }

    if Vault::open(&path).is_err() {
        Vault::ensure(&path)?;
        tracing::info!("initialized vault at {}", path.display());
    }

    registry.register(&name, &path)?;
    tracing::info!("registered vault {name} at {}", path.display());
    Ok(())
}
