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
pub struct EntityPreviewSummary {
    pub entity_id: Uuid,
    pub asset_id: Uuid,
    pub kind: String,
    pub preview_role: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelDetail {
    pub id: Uuid,
    pub title: Option<String>,
    pub description: Option<String>,
    pub follower_count: Option<String>,
    pub verified: Option<bool>,
    pub source: Option<String>,
    pub url: Option<String>,
    pub avatar_preview: Option<EntityPreviewSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityListItem {
    pub id: Uuid,
    pub title: Option<String>,
    pub mime: Option<String>,
    pub kind: Option<String>,
    pub status: String,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub preview: Option<EntityPreviewSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_sprite: Option<EntityPreviewSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_manifest: Option<EntityPreviewSummary>,
    pub primary_asset_id: Option<Uuid>,
    pub primary_asset_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_entity_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_avatar_preview: Option<EntityPreviewSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowseQuery {
    pub text: Option<String>,
    pub kind: Option<String>,
    pub source: Option<String>,
    pub exclude_source: Option<String>,
    pub status: Option<String>,
    pub limit: u32,
    pub include_hidden: bool,
    pub sort: Option<String>,
    pub uploaded_by: Option<Uuid>,
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

impl BrowseQuery {
    pub fn recent(limit: u32) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }
}
