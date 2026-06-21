use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportStrategy {
    Managed,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportManifest {
    pub source: String,
    pub vault: String,
    pub strategy: ImportStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<ManifestCollection>,
    #[serde(default)]
    pub channels: Vec<ManifestChannel>,
    #[serde(default)]
    pub items: Vec<ManifestItem>,
    #[serde(default)]
    pub membership: Vec<ManifestMembership>,
    #[serde(default)]
    pub relations: Vec<ManifestRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCollection {
    #[serde(rename = "type")]
    pub collection_type: String,
    pub external_id: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestItem {
    pub path: String,
    #[serde(default)]
    pub sha256: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<ManifestSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata_by_provenance: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSourceRef {
    pub source: String,
    pub kind: String,
    pub external_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestMembership {
    pub external_id: String,
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestChannel {
    pub source: String,
    pub kind: String,
    pub external_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestRelation {
    pub source: String,
    pub from_kind: String,
    pub from_external_id: String,
    pub to_kind: String,
    pub to_external_id: String,
    pub relation: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub entities_created: usize,
    pub entities_reused: usize,
    pub items_skipped: usize,
    pub blobs_stored: usize,
    pub blobs_deduped: usize,
    pub collection_id: Option<Uuid>,
}
