use std::fs;
use std::path::{Path, PathBuf};

use archiveos_contract::{
    VaultError, VaultMetadata, ARCHIVEOS_DIR, BLOBS_DIR, DB_FILE, STAGING_DIR, VAULT_JSON,
};
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::cas::{self, CasStoreResult};
use crate::db::{migrate, open_connection};
use crate::layout::{
    blob_path as layout_blob_path, is_dir_empty, staging_job_path, validate_layout,
};

pub struct Vault {
    root: PathBuf,
    metadata: VaultMetadata,
    db: Connection,
}

impl Vault {
    pub fn init(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root = root.as_ref().to_path_buf();

        if root.exists() {
            if !is_dir_empty(&root)? {
                return Err(VaultError::AlreadyExists);
            }
        } else {
            fs::create_dir_all(&root)?;
        }

        let archiveos = root.join(ARCHIVEOS_DIR);
        fs::create_dir_all(&archiveos)?;

        let metadata = VaultMetadata::new(Uuid::new_v4(), Utc::now());
        let vault_json = archiveos.join(VAULT_JSON);
        fs::write(&vault_json, serde_json::to_string_pretty(&metadata)?)?;

        fs::write(archiveos.join(DB_FILE), [])?;
        fs::create_dir_all(root.join(BLOBS_DIR))?;
        fs::create_dir_all(root.join(STAGING_DIR))?;

        let db = open_connection(&archiveos.join(DB_FILE))?;
        migrate(&db)?;

        Ok(Self { root, metadata, db })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root = root.as_ref().to_path_buf();
        let metadata = validate_layout(&root)?;
        let db = open_connection(&root.join(ARCHIVEOS_DIR).join(DB_FILE))?;
        migrate(&db)?;
        Ok(Self { root, metadata, db })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn id(&self) -> Uuid {
        self.metadata.id
    }

    pub fn metadata(&self) -> &VaultMetadata {
        &self.metadata
    }

    pub fn connection(&self) -> &Connection {
        &self.db
    }

    pub fn archiveos_dir(&self) -> PathBuf {
        self.root.join(ARCHIVEOS_DIR)
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.root.join(BLOBS_DIR)
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join(STAGING_DIR)
    }

    pub fn db_path(&self) -> PathBuf {
        self.archiveos_dir().join(DB_FILE)
    }

    pub fn blob_path(&self, hash: &str, ext: &str) -> Result<PathBuf, VaultError> {
        layout_blob_path(&self.root, hash, ext)
    }

    pub fn staging_job_dir(&self, job_id: &str) -> PathBuf {
        staging_job_path(&self.root, job_id)
    }

    pub fn store_managed(
        &self,
        staging_file: &Path,
        ext: &str,
    ) -> Result<CasStoreResult, VaultError> {
        cas::store_managed(&self.root, staging_file, ext)
    }

    pub fn blob_exists(&self, hash: &str, ext: &str) -> Result<bool, VaultError> {
        cas::blob_exists(&self.root, hash, ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiveos_contract::{VaultError, DB_SCHEMA_VERSION};
    use tempfile::tempdir;

    #[test]
    fn init_creates_expected_layout() {
        let parent = tempdir().unwrap();
        let vault_path = parent.path().join("my-vault");
        let vault = Vault::init(&vault_path).unwrap();

        assert!(vault_path.join(".archiveos/vault.json").is_file());
        assert!(vault_path.join(".archiveos/db.sqlite").is_file());
        assert!(vault_path.join("blobs").is_dir());
        assert!(vault_path.join("staging").is_dir());
        assert_eq!(vault.root(), vault_path.as_path());
    }

    #[test]
    fn init_applies_schema() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let version: i32 = vault
            .connection()
            .query_row(
                "SELECT MAX(version) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, DB_SCHEMA_VERSION);
    }

    #[test]
    fn init_fails_on_nonempty_dir() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("existing.txt"), b"x").unwrap();
        assert!(matches!(
            Vault::init(dir.path()),
            Err(VaultError::AlreadyExists)
        ));
    }

    #[test]
    fn open_succeeds_after_init() {
        let parent = tempdir().unwrap();
        let vault_path = parent.path().join("v");
        let v1 = Vault::init(&vault_path).unwrap();
        let v2 = Vault::open(&vault_path).unwrap();
        assert_eq!(v1.id(), v2.id());
    }

    #[test]
    fn open_runs_migrations_idempotently() {
        let dir = tempdir().unwrap();
        Vault::init(dir.path()).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let count: i32 = vault
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn open_fails_on_missing_blobs() {
        let dir = tempdir().unwrap();
        Vault::init(dir.path()).unwrap();
        std::fs::remove_dir_all(dir.path().join("blobs")).unwrap();
        assert!(Vault::open(dir.path()).is_err());
    }

    #[test]
    fn store_managed_via_vault() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let staging = vault.staging_job_dir("job-1").join("files/clip.jpg");
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::write(&staging, b"image-data").unwrap();

        let result = vault.store_managed(&staging, ".jpg").unwrap();
        assert!(!result.deduped);
        assert!(result.blob_path.starts_with(vault.blobs_dir()));
        assert!(vault.blob_exists(&result.content_hash, ".jpg").unwrap());
    }
}
