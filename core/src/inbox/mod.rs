use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use archiveos_contract::{VaultError, INBOX_DIR};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use uuid::Uuid;

use crate::cas;
use crate::import::db;
use crate::Vault;

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
    import_folder_tree(vault, folder, folder, report)?;
    Ok(())
}

/// Import `folder` and nested subfolders as a collection tree rooted at `drop_root`.
fn import_folder_tree(
    vault: &Vault,
    drop_root: &Path,
    folder: &Path,
    report: &mut InboxReport,
) -> Result<Option<Uuid>, VaultError> {
    let mut children: Vec<PathBuf> = fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| !should_skip_name(path.file_name()))
        .collect();
    children.sort();

    let mut members = Vec::new();

    for (position, path) in children.into_iter().enumerate() {
        let position = i32::try_from(position).unwrap_or(i32::MAX);
        if path.is_file() {
            let relative_path = relative_path_from_drop(drop_root, &path);
            let entity_id = import_file(vault, &path, Some(&relative_path), report)?;
            members.push((entity_id, position));
        } else if path.is_dir() {
            if let Some(child_collection_id) =
                import_folder_tree(vault, drop_root, &path, report)?
            {
                members.push((child_collection_id, position));
            }
        }
    }

    if members.is_empty() {
        if folder != drop_root {
            fs::remove_dir_all(folder)?;
        }
        return Ok(None);
    }

    let title = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("album")
        .to_string();
    let relative_path = relative_path_from_drop(drop_root, folder);
    let conn = vault.connection();
    let collection_id = create_folder_collection(conn, &title, &relative_path)?;

    for (entity_id, position) in members {
        db::link_collection_member_direct(conn, collection_id, entity_id, position)?;
    }

    fs::remove_dir_all(folder)?;
    report.collections_created += 1;
    Ok(Some(collection_id))
}

fn relative_path_from_drop(drop_root: &Path, path: &Path) -> String {
    let root_name = drop_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("album");

    if path == drop_root {
        return root_name.to_string();
    }

    let suffix = path
        .strip_prefix(drop_root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .unwrap_or_default();

    if suffix.is_empty() {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(root_name)
            .to_string()
    } else {
        format!("{root_name}/{suffix}")
    }
}

fn create_folder_collection(
    conn: &Connection,
    title: &str,
    relative_path: &str,
) -> Result<Uuid, VaultError> {
    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    db::insert_entity(conn, id, None, None, 0, "present", &now, None)?;
    db::insert_collection(conn, id, "folder", title)?;
    db::insert_source_ref(
        conn,
        Uuid::new_v4(),
        id,
        "inbox",
        "folder",
        relative_path,
        None,
        "live",
    )?;
    db::upsert_metadata(conn, id, "source_relative_path", relative_path, "system")?;
    db::upsert_metadata(conn, id, "source_kind", "inbox_folder", "system")?;
    Ok(id)
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
            "present",
            &now,
            entity_created_at,
        )?;
        report.files_imported += 1;
        id
    };

    if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
        db::upsert_metadata(conn, entity_id, "title", name, "inferred")?;
    }
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
    fn process_inbox_imports_nested_folders_as_collection_tree() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let root = vault.inbox_dir().join("parent");
        let sub_a = root.join("folder a");
        let sub_b = root.join("folder b");
        fs::create_dir_all(&sub_a).unwrap();
        fs::create_dir_all(&sub_b).unwrap();
        fs::write(sub_a.join("a.jpg"), b"a").unwrap();
        fs::write(sub_b.join("b.jpg"), b"b").unwrap();

        let report = process_inbox(&vault).unwrap();
        assert_eq!(report.files_imported, 2);
        assert_eq!(report.collections_created, 3);

        let conn = vault.connection();

        let parent_id: String = conn
            .query_row(
                "SELECT id FROM collection WHERE title = 'parent'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let child_collections: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM collection_member cm
                 JOIN collection c ON c.id = cm.entity_id
                 WHERE cm.collection_id = ?1",
                [&parent_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(child_collections, 2);

        let sub_a_files: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM collection_member cm
                 JOIN collection c ON c.id = cm.collection_id
                 WHERE c.title = 'folder a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sub_a_files, 1);

        let relative_path: String = conn
            .query_row(
                "SELECT value FROM metadata
                 WHERE key = 'source_relative_path' AND value = 'parent/folder a/a.jpg'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(relative_path, "parent/folder a/a.jpg");
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
