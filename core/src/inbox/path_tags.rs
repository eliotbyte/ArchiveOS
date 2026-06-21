use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafFolder {
    pub path: PathBuf,
    pub title: String,
    pub source_path: String,
    pub path_tags: Vec<String>,
    pub files: Vec<PathBuf>,
}

/// Discover leaf folders that directly contain importable files.
pub fn discover_leaf_folders(drop_root: &Path) -> Result<Vec<LeafFolder>, std::io::Error> {
    let mut leaves = Vec::new();
    discover_leaves_recursive(drop_root, drop_root, &mut leaves)?;
    leaves.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    Ok(leaves)
}

fn discover_leaves_recursive(
    drop_root: &Path,
    current: &Path,
    leaves: &mut Vec<LeafFolder>,
) -> Result<(), std::io::Error> {
    let mut direct_files = Vec::new();
    let mut subdirs = Vec::new();

    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if should_skip_name(path.file_name()) {
            continue;
        }
        if path.is_file() {
            direct_files.push(path);
        } else if path.is_dir() {
            subdirs.push(path);
        }
    }

    direct_files.sort();

    if !direct_files.is_empty() {
        let title = current
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("album")
            .to_string();
        leaves.push(LeafFolder {
            path: current.to_path_buf(),
            title,
            source_path: source_path_from_drop(drop_root, current),
            path_tags: path_tags_from_drop(drop_root, current),
            files: direct_files,
        });
    }

    subdirs.sort();
    for subdir in subdirs {
        discover_leaves_recursive(drop_root, &subdir, leaves)?;
    }

    Ok(())
}

pub fn source_path_from_drop(drop_root: &Path, path: &Path) -> String {
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

pub fn path_tags_from_drop(drop_root: &Path, leaf: &Path) -> Vec<String> {
    let root_name = drop_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("album");

    if leaf == drop_root {
        return Vec::new();
    }

    let relative = leaf
        .strip_prefix(drop_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    let mut tags = vec![root_name.to_string()];
    if relative.is_empty() {
        return tags;
    }

    let parts: Vec<&str> = relative.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() <= 1 {
        return tags;
    }

    tags.extend(parts[..parts.len() - 1].iter().map(|s| (*s).to_string()));
    tags
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn discovers_leaf_with_direct_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("images").join("sort").join("album1");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("peach.jpg"), b"x").unwrap();

        let drop_root = dir.path().join("images");
        let leaves = discover_leaf_folders(&drop_root).unwrap();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].title, "album1");
        assert_eq!(leaves[0].source_path, "images/sort/album1");
        assert_eq!(leaves[0].path_tags, vec!["images", "sort"]);
    }

    #[test]
    fn discovers_multiple_leaves_without_parent() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("parent");
        std::fs::create_dir_all(root.join("folder2")).unwrap();
        std::fs::create_dir_all(root.join("domik")).unwrap();
        std::fs::write(root.join("folder2/a.jpg"), b"a").unwrap();
        std::fs::write(root.join("domik/b.jpg"), b"b").unwrap();

        let leaves = discover_leaf_folders(&root).unwrap();
        assert_eq!(leaves.len(), 2);
        assert!(leaves.iter().all(|leaf| leaf.path_tags == vec!["parent"]));
    }
}
