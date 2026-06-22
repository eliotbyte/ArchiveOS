use std::path::PathBuf;
use std::time::Duration;

use archiveos_core::{open_vault_ref, Registry};
use tokio::time;

pub fn spawn(config_dir: PathBuf, vault_name: String) {
    tokio::spawn(async move {
        if let Err(err) = run_once(&config_dir, &vault_name).await {
            tracing::warn!("asset reconcile scheduler initial run failed: {err}");
        }
        let mut interval = time::interval(Duration::from_secs(reconcile_secs()));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(err) = run_once(&config_dir, &vault_name).await {
                tracing::warn!("asset reconcile scheduler poll failed: {err}");
            }
        }
    });
}

async fn run_once(
    config_dir: &PathBuf,
    vault_name: &str,
) -> Result<(), archiveos_contract::VaultError> {
    let config_dir = config_dir.clone();
    let vault_name = vault_name.to_string();
    tokio::task::spawn_blocking(move || {
        let registry = Registry::open(&config_dir)?;
        let vault = open_vault_ref(Some(&registry), &vault_name)?;
        let changed = vault.reconcile_assets()?;
        if changed > 0 {
            tracing::info!(count = changed, "asset reconcile marked missing_local assets");
        }
        Ok::<(), archiveos_contract::VaultError>(())
    })
    .await
    .expect("asset reconcile scheduler task panicked")?;
    Ok(())
}

fn reconcile_secs() -> u64 {
    std::env::var("ARCHIVEOS_ASSET_RECONCILE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&v| v > 0)
        .unwrap_or(300)
}

pub fn enabled() -> bool {
    match std::env::var("ARCHIVEOS_ASSET_RECONCILE_SCHEDULER").as_deref() {
        Ok("0") | Ok("false") | Ok("off") => false,
        Ok(_) => true,
        Err(_) => std::env::var("ARCHIVEOS_ASSET_RECONCILE_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|v| v > 0),
    }
}
