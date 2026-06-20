use std::fs;
use std::path::{Path, PathBuf};

use archiveos_contract::{VaultError, VaultRegistryEntry, VaultStatus, REGISTRY_DB};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::layout::validate_layout;
use crate::Vault;

pub struct Registry {
    config_dir: PathBuf,
    conn: Connection,
}

impl Registry {
    pub fn open(config_dir: impl AsRef<Path>) -> Result<Self, VaultError> {
        let config_dir = config_dir.as_ref().to_path_buf();
        fs::create_dir_all(&config_dir)?;

        let db_path = config_dir.join(REGISTRY_DB);
        let conn = Connection::open(&db_path).map_err(db_err)?;
        init_schema(&conn)?;

        Ok(Self { config_dir, conn })
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn register(
        &self,
        name: &str,
        path: impl AsRef<Path>,
    ) -> Result<VaultRegistryEntry, VaultError> {
        validate_name(name)?;
        let path = normalize_path(path.as_ref())?;

        if self.name_exists(name)? {
            return Err(VaultError::RegistryConflict {
                detail: format!("name already registered: {name}"),
            });
        }
        if self.path_exists(&path)? {
            return Err(VaultError::RegistryConflict {
                detail: format!("path already registered: {}", path.display()),
            });
        }

        let vault = Vault::open(&path)?;
        let status = probe_status(&path);
        let entry = VaultRegistryEntry {
            id: vault.id(),
            name: name.to_string(),
            path: path.clone(),
            status,
            registered_at: Utc::now(),
        };

        self.insert_entry(&entry)?;
        Ok(entry)
    }

    pub fn unregister(&self, name: &str) -> Result<(), VaultError> {
        let changed = self
            .conn
            .execute("DELETE FROM vault_registry WHERE name = ?1", [name])
            .map_err(db_err)?;
        if changed == 0 {
            return Err(VaultError::RegistryNotFound {
                name: name.to_string(),
            });
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<VaultRegistryEntry, VaultError> {
        self.fetch_by_name(name)?
            .ok_or_else(|| VaultError::RegistryNotFound {
                name: name.to_string(),
            })
    }

    pub fn list(&self) -> Result<Vec<VaultRegistryEntry>, VaultError> {
        self.fetch_all()
    }

    pub fn refresh(&self) -> Result<Vec<VaultRegistryEntry>, VaultError> {
        let entries = self.fetch_all()?;
        for entry in &entries {
            let status = probe_status(&entry.path);
            self.conn
                .execute(
                    "UPDATE vault_registry SET status = ?1 WHERE id = ?2",
                    params![status_as_str(status), entry.id.to_string()],
                )
                .map_err(db_err)?;
        }
        self.fetch_all()
    }

    pub fn resolve_path(&self, name: &str) -> Result<PathBuf, VaultError> {
        let entry = self.get(name)?;
        if entry.status == VaultStatus::Unavailable {
            return Err(VaultError::VaultUnavailable {
                name: name.to_string(),
            });
        }
        Ok(entry.path)
    }

    pub fn open_vault(&self, name: &str) -> Result<Vault, VaultError> {
        let path = self.resolve_path(name)?;
        Vault::open(path)
    }

    fn insert_entry(&self, entry: &VaultRegistryEntry) -> Result<(), VaultError> {
        self.conn
            .execute(
                "INSERT INTO vault_registry (id, name, path, status, registered_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    entry.id.to_string(),
                    entry.name,
                    entry.path.to_string_lossy().as_ref(),
                    status_as_str(entry.status),
                    entry.registered_at.to_rfc3339(),
                ],
            )
            .map_err(db_err)?;
        Ok(())
    }

    fn name_exists(&self, name: &str) -> Result<bool, VaultError> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM vault_registry WHERE name = ?1",
                [name],
                |_| Ok(()),
            )
            .optional()
            .map_err(db_err)?
            .is_some())
    }

    fn path_exists(&self, path: &Path) -> Result<bool, VaultError> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM vault_registry WHERE path = ?1",
                [path.to_string_lossy().as_ref()],
                |_| Ok(()),
            )
            .optional()
            .map_err(db_err)?
            .is_some())
    }

    fn fetch_by_name(&self, name: &str) -> Result<Option<VaultRegistryEntry>, VaultError> {
        self.conn
            .query_row(
                "SELECT id, name, path, status, registered_at
                 FROM vault_registry WHERE name = ?1",
                [name],
                row_to_entry,
            )
            .optional()
            .map_err(db_err)
    }

    fn fetch_all(&self) -> Result<Vec<VaultRegistryEntry>, VaultError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, path, status, registered_at
                 FROM vault_registry ORDER BY name",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], row_to_entry)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }
}

pub fn open_vault_ref(registry: Option<&Registry>, vault_ref: &str) -> Result<Vault, VaultError> {
    if looks_like_path(vault_ref) {
        return Vault::open(vault_ref);
    }

    let Some(registry) = registry else {
        return Err(VaultError::RegistryNotFound {
            name: vault_ref.to_string(),
        });
    };

    registry.open_vault(vault_ref)
}

fn looks_like_path(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute()
        || value.contains('/')
        || value.contains('\\')
        || path.extension().is_some()
}

fn validate_name(name: &str) -> Result<(), VaultError> {
    if name.is_empty() {
        return Err(VaultError::InvalidLayout {
            detail: "registry name must not be empty".into(),
        });
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(VaultError::InvalidLayout {
            detail: format!("invalid registry name: {name}"),
        });
    }
    Ok(())
}

fn normalize_path(path: &Path) -> Result<PathBuf, VaultError> {
    if !path.exists() {
        return Err(VaultError::NotFound);
    }
    path.canonicalize().map_err(VaultError::from)
}

fn probe_status(path: &Path) -> VaultStatus {
    if validate_layout(path).is_ok() {
        VaultStatus::Available
    } else {
        VaultStatus::Unavailable
    }
}

fn init_schema(conn: &Connection) -> Result<(), VaultError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS vault_registry (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL UNIQUE,
            path TEXT NOT NULL UNIQUE,
            status TEXT NOT NULL,
            registered_at TEXT NOT NULL
        );",
    )
    .map_err(db_err)
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<VaultRegistryEntry> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let path: String = row.get(2)?;
    let status: String = row.get(3)?;
    let registered_at: String = row.get(4)?;

    let status = match status.as_str() {
        "available" => VaultStatus::Available,
        "unavailable" => VaultStatus::Unavailable,
        other => {
            return Err(rusqlite::Error::InvalidColumnType(
                3,
                other.to_string(),
                rusqlite::types::Type::Text,
            ));
        }
    };

    Ok(VaultRegistryEntry {
        id: Uuid::parse_str(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        name,
        path: PathBuf::from(path),
        status,
        registered_at: registered_at.parse().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
    })
}

fn status_as_str(status: VaultStatus) -> &'static str {
    match status {
        VaultStatus::Available => "available",
        VaultStatus::Unavailable => "unavailable",
    }
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn register_lists_and_opens_by_name() {
        let config = tempdir().unwrap();
        let vault_path = tempdir().unwrap();
        Vault::init(vault_path.path()).unwrap();

        let registry = Registry::open(config.path()).unwrap();
        registry.register("main", vault_path.path()).unwrap();

        let entries = registry.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "main");
        assert_eq!(entries[0].status, VaultStatus::Available);

        let vault = registry.open_vault("main").unwrap();
        assert_eq!(vault.id(), entries[0].id);
    }

    #[test]
    fn refresh_marks_missing_vault_unavailable() {
        let config = tempdir().unwrap();
        let vault_path = tempdir().unwrap();
        Vault::init(vault_path.path()).unwrap();

        let registry = Registry::open(config.path()).unwrap();
        registry.register("gone", vault_path.path()).unwrap();

        fs::remove_dir_all(vault_path.path()).unwrap();
        let entries = registry.refresh().unwrap();
        assert_eq!(entries[0].status, VaultStatus::Unavailable);

        assert!(matches!(
            registry.open_vault("gone"),
            Err(VaultError::VaultUnavailable { .. })
        ));
    }

    #[test]
    fn open_vault_ref_resolves_registry_name() {
        let config = tempdir().unwrap();
        let vault_path = tempdir().unwrap();
        Vault::init(vault_path.path()).unwrap();

        let registry = Registry::open(config.path()).unwrap();
        registry.register("pics", vault_path.path()).unwrap();

        let vault = open_vault_ref(Some(&registry), "pics").unwrap();
        assert_eq!(vault.root(), vault_path.path().canonicalize().unwrap());
    }

    #[test]
    fn register_rejects_duplicate_name() {
        let config = tempdir().unwrap();
        let v1 = tempdir().unwrap();
        let v2 = tempdir().unwrap();
        Vault::init(v1.path()).unwrap();
        Vault::init(v2.path()).unwrap();

        let registry = Registry::open(config.path()).unwrap();
        registry.register("dup", v1.path()).unwrap();
        assert!(matches!(
            registry.register("dup", v2.path()),
            Err(VaultError::RegistryConflict { .. })
        ));
    }
}
