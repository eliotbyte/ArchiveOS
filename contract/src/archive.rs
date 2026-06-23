use serde::{Deserialize, Serialize};

use crate::VaultError;

fn default_true() -> bool {
    true
}

fn default_mode_once() -> String {
    "once".into()
}

fn default_removed_items() -> String {
    "mark_removed".into()
}

fn default_video() -> String {
    "best".into()
}

fn default_subtitles() -> String {
    "preferred".into()
}

fn default_audio_tracks() -> String {
    "main".into()
}

fn default_subtitle_languages() -> Vec<String> {
    vec!["original".into(), "en".into(), "ru".into()]
}

/// Controls which media ArchiveOS archives for a yt-dlp job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPolicy {
    #[serde(default = "default_video")]
    pub video: String,
    #[serde(default = "default_true")]
    pub thumbnail: bool,
    #[serde(default = "default_true")]
    pub channel_avatar: bool,
    #[serde(default = "default_subtitles")]
    pub subtitles: String,
    #[serde(default = "default_subtitle_languages")]
    pub subtitle_languages: Vec<String>,
    #[serde(default = "default_true")]
    pub automatic_subtitles: bool,
    #[serde(default = "default_audio_tracks")]
    pub audio_tracks: String,
    #[serde(default)]
    pub audio_languages: Vec<String>,
}

impl Default for AssetPolicy {
    fn default() -> Self {
        Self {
            video: default_video(),
            thumbnail: default_true(),
            channel_avatar: default_true(),
            subtitles: default_subtitles(),
            subtitle_languages: default_subtitle_languages(),
            automatic_subtitles: default_true(),
            audio_tracks: default_audio_tracks(),
            audio_languages: Vec::new(),
        }
    }
}

/// Structured yt-dlp job input stored in `job.input`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct YtdlpJobInput {
    pub url: String,
    #[serde(default = "default_mode_once")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub resync: bool,
    #[serde(default = "default_removed_items")]
    pub removed_items: String,
    #[serde(default)]
    pub asset_policy: AssetPolicy,
}

impl YtdlpJobInput {
    pub fn once(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            mode: default_mode_once(),
            resync: true,
            removed_items: default_removed_items(),
            asset_policy: AssetPolicy::default(),
        }
    }

    pub fn subscription(url: impl Into<String>, asset_policy: AssetPolicy) -> Self {
        Self {
            url: url.into(),
            mode: "subscription".into(),
            resync: true,
            removed_items: default_removed_items(),
            asset_policy,
        }
    }

    pub fn to_json(&self) -> Result<String, VaultError> {
        serde_json::to_string(self).map_err(VaultError::from)
    }

    /// Accept legacy plain URL strings or structured JSON objects.
    pub fn normalize(input: &str) -> Result<Self, VaultError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(VaultError::InvalidLayout {
                detail: "job input is empty".into(),
            });
        }
        if trimmed.starts_with('{') {
            return serde_json::from_str(trimmed).map_err(VaultError::from);
        }
        Ok(Self::once(trimmed))
    }
}

/// Options persisted on a source subscription row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionOptions {
    #[serde(default = "default_true")]
    pub resync: bool,
    #[serde(default = "default_removed_items")]
    pub removed_items: String,
    #[serde(default)]
    pub asset_policy: AssetPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_entity_id: Option<String>,
}

impl Default for SubscriptionOptions {
    fn default() -> Self {
        Self {
            resync: default_true(),
            removed_items: default_removed_items(),
            asset_policy: AssetPolicy::default(),
            collection_entity_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_legacy_url() {
        let input = YtdlpJobInput::normalize("https://youtube.com/watch?v=abc").unwrap();
        assert_eq!(input.url, "https://youtube.com/watch?v=abc");
        assert_eq!(input.mode, "once");
        assert_eq!(input.asset_policy.video, "best");
    }

    #[test]
    fn normalize_structured_json() {
        let raw = r#"{"url":"https://example.com","mode":"once","asset_policy":{"video":"none"}}"#;
        let input = YtdlpJobInput::normalize(raw).unwrap();
        assert_eq!(input.asset_policy.video, "none");
    }

    #[test]
    fn roundtrip_json() {
        let input = YtdlpJobInput::once("https://example.com");
        let parsed = YtdlpJobInput::normalize(&input.to_json().unwrap()).unwrap();
        assert_eq!(parsed, input);
    }
}
