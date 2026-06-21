use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityHit {
    pub id: Uuid,
    pub content_hash: Option<String>,
    pub mime: Option<String>,
    pub title: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataEntry {
    pub key: String,
    pub value: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub tag: Option<String>,
    pub text: Option<String>,
    pub include_hidden: bool,
}

impl SearchQuery {
    pub fn tag(name: impl Into<String>) -> Self {
        Self {
            tag: Some(name.into()),
            text: None,
            include_hidden: false,
        }
    }

    pub fn text(query: impl Into<String>) -> Self {
        Self {
            tag: None,
            text: Some(query.into()),
            include_hidden: false,
        }
    }
}
