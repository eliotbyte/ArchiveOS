use std::path::PathBuf;
use std::time::Duration;

use archiveos_core::{open_vault_ref, Registry};
use tokio::time;

pub fn spawn(config_dir: PathBuf, vault_name: String) {
    tokio::spawn(async move {
        if let Err(err) = run_once(&config_dir, &vault_name).await {
            tracing::warn!("blob gc scheduler initial run failed: {err}");
        }
        let mut interval = time::interval(Duration::from_secs(gc_secs()));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(err) = run_once(&config_dir, &vault_name).await {
                tracing::warn!("blob gc scheduler poll failed: {err}");
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
        let report = vault.reconcile_blobs(false, min_age_secs())?;
        if report.removed > 0 {
            tracing::info!(
                removed = report.removed,
                bytes_freed = report.bytes_freed,
                "blob gc removed unreferenced blobs"
            );
        }
        Ok::<(), archiveos_contract::VaultError>(())
    })
    .await
    .expect("blob gc scheduler task panicked")?;
    Ok(())
}

fn gc_secs() -> u64 {
    std::env::var("ARCHIVEOS_BLOB_GC_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&v| v > 0)
        .unwrap_or(86_400)
}

fn min_age_secs() -> u64 {
    std::env::var("ARCHIVEOS_BLOB_GC_MIN_AGE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(86_400)
}

pub fn enabled() -> bool {
    match std::env::var("ARCHIVEOS_BLOB_GC_SCHEDULER").as_deref() {
        Ok("0") | Ok("false") | Ok("off") => false,
        Ok(_) => true,
        Err(_) => std::env::var("ARCHIVEOS_BLOB_GC_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|v| v > 0),
    }
}
