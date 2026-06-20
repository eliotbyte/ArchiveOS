use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use archiveos_contract::{ImportManifest, ImportStrategy};
use archiveos_core::Vault;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "archive-os", about = "ArchiveOS personal media vault")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
}

#[derive(Subcommand)]
enum VaultAction {
    /// Create a new vault at PATH
    Init { path: PathBuf },
    /// Validate and open an existing vault at PATH
    Open { path: PathBuf },
    /// Index files from DIR into VAULT (reference strategy, no manifest)
    Scan { vault: PathBuf, dir: PathBuf },
    /// Import a staging job directory using manifest.json (managed)
    Import {
        vault: PathBuf,
        staging: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Vault { action } => match action {
            VaultAction::Init { path } => {
                let vault = Vault::init(&path).context("failed to init vault")?;
                println!("vault_id={}", vault.id());
            }
            VaultAction::Open { path } => {
                let vault = Vault::open(&path).context("failed to open vault")?;
                let root = vault
                    .root()
                    .canonicalize()
                    .unwrap_or_else(|_| vault.root().to_path_buf());
                println!("vault_id={}", vault.id());
                println!("root={}", root.display());
                println!("blobs={}", vault.blobs_dir().display());
                println!("staging={}", vault.staging_dir().display());
            }
            VaultAction::Scan { vault, dir } => {
                let vault = Vault::open(&vault).context("failed to open vault")?;
                let report = vault
                    .import(&dir, None, ImportStrategy::Reference)
                    .context("scan import failed")?;
                print_report(&report);
            }
            VaultAction::Import {
                vault,
                staging,
                manifest,
            } => {
                let vault = Vault::open(&vault).context("failed to open vault")?;
                let manifest_path = manifest.unwrap_or_else(|| staging.join("manifest.json"));
                let contents =
                    fs::read_to_string(&manifest_path).context("failed to read manifest")?;
                let manifest: ImportManifest =
                    serde_json::from_str(&contents).context("invalid manifest json")?;
                let report = vault
                    .import(&staging, Some(&manifest), ImportStrategy::Managed)
                    .context("managed import failed")?;
                print_report(&report);
            }
        },
    }
    Ok(())
}

fn print_report(report: &archiveos_contract::ImportReport) {
    println!("entities_created={}", report.entities_created);
    println!("entities_reused={}", report.entities_reused);
    println!("items_skipped={}", report.items_skipped);
    println!("blobs_stored={}", report.blobs_stored);
    println!("blobs_deduped={}", report.blobs_deduped);
    if let Some(id) = report.collection_id {
        println!("collection_id={id}");
    }
}
