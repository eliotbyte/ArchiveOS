mod error;
mod routes;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use archiveos_core::{Registry, Vault};
use axum::Router;
use tokio::net::TcpListener;
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

    let state = Arc::new(AppState { config_dir });
    let app = Router::new()
        .merge(routes::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(addr).await.expect("bind failed");
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await.expect("server failed");
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
        Vault::init(&path)?;
        tracing::info!("initialized vault at {}", path.display());
    }

    registry.register(&name, &path)?;
    tracing::info!("registered vault {name} at {}", path.display());
    Ok(())
}
