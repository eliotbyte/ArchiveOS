use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use archiveos_contract::{ImportManifest, ImportStrategy, SearchQuery};
use archiveos_core::{open_vault_ref, Registry, Vault};
use clap::{Parser, Subcommand};
use uuid::Uuid;

const DEFAULT_CONFIG_DIR: &str = "config";

#[derive(Parser)]
#[command(name = "archive-os", about = "ArchiveOS personal media vault")]
struct Cli {
    /// Config directory (registry.sqlite lives here)
    #[arg(long, global = true, default_value = DEFAULT_CONFIG_DIR)]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },
    Search {
        vault: String,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        query: Option<String>,
    },
}

#[derive(Subcommand)]
enum TagAction {
    /// Attach tag to entity
    Add {
        vault: String,
        entity: String,
        name: String,
    },
    /// Detach tag from entity
    Remove {
        vault: String,
        entity: String,
        name: String,
    },
}

#[derive(Subcommand)]
enum RegistryAction {
    /// List registered vaults
    List,
    /// Register an existing vault by name
    Add { name: String, path: PathBuf },
    /// Remove a vault from the registry (does not delete files)
    Remove { name: String },
    /// Re-probe vault paths and update availability status
    Refresh,
}

#[derive(Subcommand)]
enum VaultAction {
    /// Create a new vault at PATH
    Init {
        path: PathBuf,
        /// Register under this name after init
        #[arg(long)]
        name: Option<String>,
    },
    /// Open vault by registry NAME or filesystem PATH
    Open { vault: String },
    /// Index files from DIR into VAULT (name or path)
    Scan { vault: String, dir: PathBuf },
    /// Import staging job using manifest.json (managed)
    Import {
        vault: String,
        staging: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let registry = Registry::open(&cli.config).context("failed to open registry")?;

    match cli.command {
        Command::Registry { action } => match action {
            RegistryAction::List => {
                let entries = registry.refresh().context("failed to list vaults")?;
                for entry in entries {
                    println!(
                        "name={} id={} status={:?} path={}",
                        entry.name,
                        entry.id,
                        entry.status,
                        entry.path.display()
                    );
                }
            }
            RegistryAction::Add { name, path } => {
                let entry = registry
                    .register(&name, &path)
                    .context("failed to register vault")?;
                println!("name={} id={} path={}", entry.name, entry.id, entry.path.display());
            }
            RegistryAction::Remove { name } => {
                registry.unregister(&name).context("failed to remove vault")?;
                println!("removed={name}");
            }
            RegistryAction::Refresh => {
                let entries = registry.refresh().context("failed to refresh registry")?;
                for entry in entries {
                    println!("name={} status={:?}", entry.name, entry.status);
                }
            }
        },
        Command::Tag { action } => match action {
            TagAction::Add { vault, entity, name } => {
                let vault = open_vault_ref(Some(&registry), &vault)
                    .context("failed to open vault")?;
                let entity_id = Uuid::parse_str(&entity).context("invalid entity uuid")?;
                vault.add_tag(entity_id, &name).context("failed to add tag")?;
                println!("tagged entity={entity_id} tag={name}");
            }
            TagAction::Remove { vault, entity, name } => {
                let vault = open_vault_ref(Some(&registry), &vault)
                    .context("failed to open vault")?;
                let entity_id = Uuid::parse_str(&entity).context("invalid entity uuid")?;
                vault
                    .remove_tag(entity_id, &name)
                    .context("failed to remove tag")?;
                println!("untagged entity={entity_id} tag={name}");
            }
        },
        Command::Search { vault, tag, query } => {
            let vault = open_vault_ref(Some(&registry), &vault)
                .context("failed to open vault")?;
            let hits = vault
                .search(&SearchQuery {
                    tag,
                    text: query,
                    include_hidden: false,
                })
                .context("search failed")?;
            for hit in hits {
                let tags = hit.tags.join(",");
                let title = hit.title.unwrap_or_default();
                println!(
                    "entity_id={} title={title} mime={} tags={tags}",
                    hit.id,
                    hit.mime.unwrap_or_default(),
                );
            }
        },
        Command::Vault { action } => match action {
            VaultAction::Init { path, name } => {
                let vault = Vault::init(&path).context("failed to init vault")?;
                println!("vault_id={}", vault.id());
                if let Some(name) = name {
                    let entry = registry
                        .register(&name, vault.root())
                        .context("failed to register vault")?;
                    println!("registered name={} id={}", entry.name, entry.id);
                }
            }
            VaultAction::Open { vault } => {
                let vault = open_vault_ref(Some(&registry), &vault)
                    .context("failed to open vault")?;
                print_vault_info(&vault);
            }
            VaultAction::Scan { vault, dir } => {
                let vault = open_vault_ref(Some(&registry), &vault)
                    .context("failed to open vault")?;
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
                let vault = open_vault_ref(Some(&registry), &vault)
                    .context("failed to open vault")?;
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

fn print_vault_info(vault: &Vault) {
    let root = vault
        .root()
        .canonicalize()
        .unwrap_or_else(|_| vault.root().to_path_buf());
    println!("vault_id={}", vault.id());
    println!("root={}", root.display());
    println!("blobs={}", vault.blobs_dir().display());
    println!("staging={}", vault.staging_dir().display());
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
