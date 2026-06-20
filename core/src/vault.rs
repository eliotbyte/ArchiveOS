//! Vault lifecycle.

use std::path::{Path, PathBuf};

pub struct Vault {
    root: PathBuf,
}

impl Vault {
    pub fn root(&self) -> &Path {
        &self.root
    }
}
