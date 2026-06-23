use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::EntityPreviewSummary;

/// Unified list kind for smart, user, and source collections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryListKind {
    Smart,
    User,
    Source,
}

/// Smart list identifiers (virtual, not stored as collections).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartListId {
    RecentlyAdded,
    ContinueWatching,
    RecentlyWatched,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryListQuery {
    pub section: Option<String>,
    pub source: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryListSummary {
    pub id: String,
    pub list_kind: LibraryListKind,
    pub list_type: String,
    pub title: String,
    pub member_count: i32,
    pub cover_preview: Option<EntityPreviewSummary>,
    pub icon: Option<String>,
    pub overlay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryListDetail {
    pub id: String,
    pub list_kind: LibraryListKind,
    pub list_type: String,
    pub title: String,
    pub member_count: i32,
    pub cover_preview: Option<EntityPreviewSummary>,
    pub icon: Option<String>,
    pub overlay: bool,
    pub members: Vec<LibraryListMember>,
    pub sort_options: Vec<String>,
    pub default_sort: String,
    pub can_reorder: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryListMember {
    pub id: Uuid,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub mime: Option<String>,
    pub source: Option<String>,
    pub status: String,
    pub position: i32,
    pub preview: Option<EntityPreviewSummary>,
    pub timeline_sprite: Option<EntityPreviewSummary>,
    pub timeline_manifest: Option<EntityPreviewSummary>,
    pub primary_asset_id: Option<Uuid>,
    pub primary_asset_status: Option<String>,
    pub duration: Option<String>,
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_entity_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_avatar_preview: Option<EntityPreviewSummary>,
    pub uploader: Option<String>,
    pub webpage_url: Option<String>,
    pub playback_position: Option<f64>,
    pub playback_progress: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackStateInput {
    pub asset_id: Uuid,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackStateResponse {
    pub entity_id: Uuid,
    pub asset_id: Uuid,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserListInput {
    pub title: String,
    pub list_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddUserListMemberInput {
    pub entity_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderUserListMembersInput {
    pub entity_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowseSort {
    AddedDesc,
    PublishedDesc,
    ViewsDesc,
    Manual,
}

impl BrowseSort {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "added_desc" => Some(Self::AddedDesc),
            "published_desc" => Some(Self::PublishedDesc),
            "views_desc" => Some(Self::ViewsDesc),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AddedDesc => "added_desc",
            Self::PublishedDesc => "published_desc",
            Self::ViewsDesc => "views_desc",
            Self::Manual => "manual",
        }
    }
}
