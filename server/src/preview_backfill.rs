use std::path::PathBuf;

use archiveos_core::{open_vault_ref, Registry};

pub fn spawn(config_dir: PathBuf, vault_name: String) {
    tokio::spawn(async move {
        if let Err(err) = run_once(&config_dir, &vault_name).await {
            tracing::warn!("preview backfill failed: {err}");
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
        let report = vault.backfill_preview_jobs(&vault_name)?;
        if report.queued > 0 {
            tracing::info!(
                scanned = report.scanned,
                queued = report.queued,
                skipped = report.skipped,
                "preview backfill queued jobs"
            );
        }
        Ok::<(), archiveos_contract::VaultError>(())
    })
    .await
    .expect("preview backfill task panicked")?;
    Ok(())
}

pub fn enabled() -> bool {
    match std::env::var("ARCHIVEOS_PREVIEW_BACKFILL_ON_START").as_deref() {
        Ok("1") | Ok("true") | Ok("on") => true,
        _ => false,
    }
}
