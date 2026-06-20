use std::path::PathBuf;

use anyhow::{Context, Result};
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
        },
    }
    Ok(())
}
