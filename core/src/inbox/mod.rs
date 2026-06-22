mod fingerprint;
mod path_tags;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use archiveos_contract::{VaultError, INBOX_DIR};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use uuid::Uuid;

use crate::assets::UpsertAssetInput;
use crate::cas;
use crate::import::db;
use crate::tags::add_tag;
use crate::Vault;

pub use fingerprint::compute_collection_fingerprint;
pub use path_tags::{discover_leaf_folders, LeafFolder};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InboxReport {
    pub files_imported: u32,
    pub collections_created: u32,
    pub items_skipped: u32,
    pub entities_reused: u32,
}

pub fn ensure_inbox_dir(root: &Path) -> Result<PathBuf, VaultError> {
    let inbox = root.join(INBOX_DIR);
    fs::create_dir_all(&inbox)?;
    Ok(inbox)
}

pub fn process_inbox(vault: &Vault) -> Result<InboxReport, VaultError> {
    let inbox = ensure_inbox_dir(vault.root())?;
    let mut report = InboxReport::default();
    let mut entries: Vec<PathBuf> = fs::read_dir(&inbox)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    entries.sort();

    for path in entries {
        if path.is_file() {
            if should_skip_name(path.file_name()) {
                report.items_skipped += 1;
                continue;
            }
            import_file(vault, &path, None, &mut report)?;
        } else if path.is_dir() {
            if should_skip_name(path.file_name()) {
                report.items_skipped += 1;
                continue;
            }
            import_folder(vault, &path, &mut report)?;
        }
    }

    Ok(report)
}

fn import_folder(vault: &Vault, folder: &Path, report: &mut InboxReport) -> Result<(), VaultError> {
    let leaves = discover_leaf_folders(folder).map_err(|err| VaultError::Io(err))?;
    if leaves.is_empty() {
        fs::remove_dir_all(folder)?;
        return Ok(());
    }

    for leaf in leaves {
        import_leaf_folder(vault, folder, &leaf, report)?;
    }

    fs::remove_dir_all(folder)?;
    Ok(())
}

fn import_leaf_folder(
    vault: &Vault,
    drop_root: &Path,
    leaf: &LeafFolder,
    report: &mut InboxReport,
) -> Result<(), VaultError> {
    let mut members = Vec::new();
    let mut file_hashes = Vec::new();

    for (position, file_path) in leaf.files.iter().enumerate() {
        let file_source_path = file_source_path(drop_root, file_path);
        let entity_id = import_file(vault, file_path, Some(&file_source_path), report)?;
        members.push((entity_id, i32::try_from(position).unwrap_or(i32::MAX)));

        let hash: String = vault
            .connection()
            .query_row(
                "SELECT content_hash FROM entity WHERE id = ?1",
                [entity_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|err| VaultError::Database {
                detail: err.to_string(),
            })?;
        file_hashes.push(hash);
    }

    let fingerprint = compute_collection_fingerprint(&file_hashes);
    let conn = vault.connection();
    let collection_id = find_or_create_collection_by_fingerprint(
        conn,
        &leaf.title,
        &fingerprint,
        &leaf.source_path,
        &leaf.path_tags,
        report,
    )?;

    for (entity_id, position) in members {
        db::link_collection_member_direct(conn, collection_id, entity_id, position)?;
    }

    Ok(())
}

fn find_or_create_collection_by_fingerprint(
    conn: &Connection,
    title: &str,
    fingerprint: &str,
    source_path: &str,
    path_tags: &[String],
    report: &mut InboxReport,
) -> Result<Uuid, VaultError> {
    if let Some(existing) = db::find_collection_by_fingerprint(conn, fingerprint)? {
        db::add_collection_source_path(conn, existing, source_path)?;
        apply_path_tags(conn, existing, path_tags)?;
        db::upsert_metadata(conn, existing, "source_relative_path", source_path, "system")?;
        return Ok(existing);
    }

    let id = create_folder_collection(conn, title, source_path, fingerprint)?;
    apply_path_tags(conn, id, path_tags)?;
    report.collections_created += 1;
    Ok(id)
}

fn create_folder_collection(
    conn: &Connection,
    title: &str,
    source_path: &str,
    fingerprint: &str,
) -> Result<Uuid, VaultError> {
    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    db::insert_entity(conn, id, None, None, 0, "active", &now, None)?;
    db::insert_collection(conn, id, "folder", title)?;
    db::set_collection_fingerprint(conn, id, fingerprint)?;
    db::add_collection_source_path(conn, id, source_path)?;
    db::upsert_metadata(conn, id, "source_relative_path", source_path, "system")?;
    db::upsert_metadata(conn, id, "source_kind", "inbox_folder", "system")?;
    Ok(id)
}

fn apply_path_tags(conn: &Connection, collection_id: Uuid, path_tags: &[String]) -> Result<(), VaultError> {
    for tag in path_tags {
        add_tag(conn, collection_id, tag)?;
    }
    Ok(())
}

fn file_source_path(drop_root: &Path, file_path: &Path) -> String {
    path_tags::source_path_from_drop(drop_root, file_path.parent().unwrap_or(drop_root))
        + "/"
        + file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
}

fn import_file(
    vault: &Vault,
    file_path: &Path,
    source_relative_path: Option<&str>,
    report: &mut InboxReport,
) -> Result<Uuid, VaultError> {
    let meta = fs::metadata(file_path)?;
    let created_at = file_created_at(&meta);
    let modified_at = file_modified_at(&meta);
    let image_meta = crate::media::image::extract(file_path);
    let entity_created_at = image_meta
        .taken_at
        .as_deref()
        .or(created_at.as_deref());
    let ext = extension_from_path(file_path);
    let cas = cas::store_managed(vault.root(), file_path, &ext)?;
    let mime = guess_mime(&cas.blob_path);
    let conn = vault.connection();

    let entity_id = if let Some(existing) = db::find_entity_by_content_hash(conn, &cas.content_hash)? {
        report.entities_reused += 1;
        db::update_entity_content(conn, existing, &cas.content_hash, &mime, cas.size)?;
        existing
    } else {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        db::insert_entity(
            conn,
            id,
            Some(&cas.content_hash),
            Some(&mime),
            cas.size,
            "active",
            &now,
            entity_created_at,
        )?;
        report.files_imported += 1;
        id
    };

    if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
        db::upsert_metadata(conn, entity_id, "title", name, "inferred")?;
    }
    let asset_path = cas.blob_path.to_string_lossy().into_owned();
    crate::assets::upsert_asset(
        conn,
        &UpsertAssetInput {
            entity_id,
            role: "primary",
            kind: "file",
            content_hash: Some(&cas.content_hash),
            mime: Some(&mime),
            size: cas.size,
            ext: Some(&ext),
            status: "present",
            storage_strategy: "managed",
            path: Some(&asset_path),
        },
    )?;
    if let Some(relative_path) = source_relative_path {
        db::upsert_metadata(conn, entity_id, "source_relative_path", relative_path, "system")?;
    }
    crate::media::image::persist(
        conn,
        entity_id,
        &image_meta,
        modified_at.as_deref(),
    )?;

    Ok(entity_id)
}

fn should_skip_name(name: Option<&std::ffi::OsStr>) -> bool {
    let Some(name) = name.and_then(|n| n.to_str()) else {
        return true;
    };
    if name.starts_with('.') {
        return true;
    }
    matches!(
        name.to_ascii_lowercase().as_str(),
        "thumbs.db" | "desktop.ini" | ".ds_store"
    )
}

fn extension_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default()
}

fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

fn file_created_at(meta: &fs::Metadata) -> Option<String> {
    meta.created()
        .or_else(|_| meta.modified())
        .ok()
        .and_then(system_time_to_rfc3339)
}

fn file_modified_at(meta: &fs::Metadata) -> Option<String> {
    meta.modified()
        .ok()
        .and_then(system_time_to_rfc3339)
}

fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    Some(DateTime::<Utc>::from(time).to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;
    use walkdir::WalkDir;

    fn count_collections(vault: &Vault) -> i32 {
        vault
            .connection()
            .query_row("SELECT COUNT(*) FROM collection", [], |row| row.get(0))
            .unwrap()
    }

    fn count_source_refs_for_collection(vault: &Vault, collection_id: Uuid) -> i32 {
        vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM source_ref WHERE entity_id = ?1 AND kind = 'folder'",
                [collection_id.to_string()],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn collection_tags(vault: &Vault, collection_id: Uuid) -> Vec<String> {
        crate::tags::list_entity_tags(vault.connection(), collection_id).unwrap()
    }

    #[test]
    fn process_inbox_imports_file_into_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let inbox = vault.inbox_dir();
        fs::write(inbox.join("photo.jpg"), b"jpeg-bytes").unwrap();

        let report = process_inbox(&vault).unwrap();
        assert_eq!(report.files_imported, 1);
        assert_eq!(report.collections_created, 0);
        assert!(fs::read_dir(inbox).unwrap().next().is_none());
        assert!(WalkDir::new(vault.blobs_dir())
            .into_iter()
            .filter_map(|e| e.ok())
            .any(|e| e.file_type().is_file()));
    }

    #[test]
    fn process_inbox_imports_flat_folder_as_collection() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let album = vault.inbox_dir().join("vacation");
        fs::create_dir_all(&album).unwrap();
        fs::write(album.join("a.jpg"), b"a").unwrap();
        fs::write(album.join("b.jpg"), b"b").unwrap();

        let report = process_inbox(&vault).unwrap();
        assert_eq!(report.files_imported, 2);
        assert_eq!(report.collections_created, 1);
        assert!(!vault.inbox_dir().join("vacation").exists());

        let count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM collection_member cm
                 JOIN collection c ON c.id = cm.collection_id
                 WHERE c.title = 'vacation'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn process_inbox_merges_same_content_from_different_paths() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let first = vault.inbox_dir().join("images/sort/album1");
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("peach.jpg"), b"same").unwrap();
        process_inbox(&vault).unwrap();

        let second = vault.inbox_dir().join("images/album1");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("peach.jpg"), b"same").unwrap();
        let report = process_inbox(&vault).unwrap();

        assert_eq!(report.collections_created, 0);
        assert_eq!(count_collections(&vault), 1);

        let collection_id: Uuid = vault
            .connection()
            .query_row("SELECT id FROM collection LIMIT 1", [], |row| {
                let id: String = row.get(0)?;
                Ok(Uuid::parse_str(&id).unwrap())
            })
            .unwrap();
        assert_eq!(count_source_refs_for_collection(&vault, collection_id), 2);
        assert_eq!(
            collection_tags(&vault, collection_id),
            vec!["images".to_string(), "sort".to_string()]
        );
    }

    #[test]
    fn process_inbox_keeps_same_title_with_different_content_separate() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let first = vault.inbox_dir().join("album1");
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("peach.jpg"), b"peach").unwrap();
        process_inbox(&vault).unwrap();

        let second = vault.inbox_dir().join("album1");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("banana.jpg"), b"banana").unwrap();
        let report = process_inbox(&vault).unwrap();

        assert_eq!(report.collections_created, 1);
        assert_eq!(count_collections(&vault), 2);
    }

    #[test]
    fn process_inbox_creates_leaf_albums_without_parent_collection() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let root = vault.inbox_dir().join("parent");
        fs::create_dir_all(root.join("folder2")).unwrap();
        fs::create_dir_all(root.join("domik")).unwrap();
        fs::write(root.join("folder2/a.jpg"), b"a").unwrap();
        fs::write(root.join("domik/b.jpg"), b"b").unwrap();

        let report = process_inbox(&vault).unwrap();
        assert_eq!(report.files_imported, 2);
        assert_eq!(report.collections_created, 2);
        assert_eq!(count_collections(&vault), 2);

        let tagged_parent: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(DISTINCT c.id) FROM collection c
                 JOIN entity_tag et ON et.entity_id = c.id
                 JOIN tag t ON t.id = et.tag_id
                 WHERE t.name = 'parent'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tagged_parent, 2);
    }

    #[test]
    fn process_inbox_different_file_sets_do_not_merge() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let first = vault.inbox_dir().join("album1");
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("a.jpg"), b"a").unwrap();
        process_inbox(&vault).unwrap();

        let second = vault.inbox_dir().join("album1");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("a.jpg"), b"a").unwrap();
        fs::write(second.join("b.jpg"), b"b").unwrap();
        let report = process_inbox(&vault).unwrap();

        assert_eq!(report.collections_created, 1);
        assert_eq!(count_collections(&vault), 2);
    }

    #[test]
    fn process_inbox_reuses_existing_files_and_albums() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let loose = vault.inbox_dir().join("dup.jpg");
        fs::write(&loose, b"same-bytes").unwrap();
        process_inbox(&vault).unwrap();

        let album = vault.inbox_dir().join("my album");
        fs::create_dir_all(&album).unwrap();
        fs::write(album.join("dup.jpg"), b"same-bytes").unwrap();
        let report = process_inbox(&vault).unwrap();

        assert_eq!(report.files_imported, 0);
        assert_eq!(report.entities_reused, 1);

        let entity_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entity WHERE content_hash IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(entity_count, 1);

        let membership_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM collection_member",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(membership_count, 1);
    }

    #[test]
    fn process_inbox_reuses_existing_album_by_content_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let first = vault.inbox_dir().join("sort/vacation");
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("a.jpg"), b"a").unwrap();
        process_inbox(&vault).unwrap();

        let second = vault.inbox_dir().join("vacation");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("a.jpg"), b"a").unwrap();
        let report = process_inbox(&vault).unwrap();

        assert_eq!(report.collections_created, 0);
        assert_eq!(count_collections(&vault), 1);

        let collection_id: Uuid = vault
            .connection()
            .query_row("SELECT id FROM collection LIMIT 1", [], |row| {
                let id: String = row.get(0)?;
                Ok(Uuid::parse_str(&id).unwrap())
            })
            .unwrap();
        assert_eq!(count_source_refs_for_collection(&vault, collection_id), 2);
    }

    #[test]
    fn process_inbox_skips_junk_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        fs::write(vault.inbox_dir().join("Thumbs.db"), b"x").unwrap();
        fs::write(vault.inbox_dir().join("keep.png"), b"png").unwrap();

        let report = process_inbox(&vault).unwrap();
        assert_eq!(report.files_imported, 1);
        assert_eq!(report.items_skipped, 1);
        assert!(vault.inbox_dir().join("Thumbs.db").exists());
    }
}
