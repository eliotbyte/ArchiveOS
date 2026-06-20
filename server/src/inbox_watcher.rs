use std::path::PathBuf;
use std::time::Duration;

use archiveos_core::{open_vault_ref, Registry};
use tokio::time;

pub fn spawn(config_dir: PathBuf, vault_name: String) {
    tokio::spawn(async move {
        if let Err(err) = run_once(&config_dir, &vault_name).await {
            tracing::warn!("inbox initial scan failed: {err}");
        }
        let mut interval = time::interval(Duration::from_secs(poll_secs()));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(err) = run_once(&config_dir, &vault_name).await {
                tracing::warn!("inbox poll failed: {err}");
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
        let report = vault.process_inbox()?;
        if report.files_imported > 0
            || report.collections_created > 0
            || report.items_skipped > 0
        {
            tracing::info!(
                files = report.files_imported,
                collections = report.collections_created,
                skipped = report.items_skipped,
                reused = report.entities_reused,
                "inbox processed"
            );
        }
        Ok::<(), archiveos_contract::VaultError>(())
    })
    .await
    .expect("inbox worker task panicked")?;
    Ok(())
}

fn poll_secs() -> u64 {
    std::env::var("ARCHIVEOS_INBOX_POLL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&v| v > 0)
        .unwrap_or(15)
}

pub fn enabled() -> bool {
    match std::env::var("ARCHIVEOS_INBOX_WATCH").as_deref() {
        Ok("0") | Ok("false") | Ok("off") => false,
        Ok(_) => true,
        Err(_) => std::env::var("ARCHIVEOS_DEFAULT_VAULT")
            .map(|v| !v.is_empty())
            .unwrap_or(false),
    }
}
