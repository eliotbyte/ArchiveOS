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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub tag: Option<String>,
    pub text: Option<String>,
}

impl SearchQuery {
    pub fn tag(name: impl Into<String>) -> Self {
        Self {
            tag: Some(name.into()),
            text: None,
        }
    }

    pub fn text(query: impl Into<String>) -> Self {
        Self {
            tag: None,
            text: Some(query.into()),
        }
    }
}
