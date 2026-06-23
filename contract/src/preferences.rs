use serde::{Deserialize, Serialize};

use crate::AssetPolicy;

fn default_subtitle_languages() -> Vec<String> {
    vec!["ru".into(), "en".into(), "original".into()]
}

fn default_subtitle_mode() -> String {
    "preferred".into()
}

fn default_audio_languages() -> Vec<String> {
    vec!["original".into(), "ru".into(), "en".into()]
}

fn default_audio_mode() -> String {
    "preferred".into()
}

fn default_video_quality() -> String {
    "best".into()
}

fn default_true() -> bool {
    true
}

/// Per-user media download defaults (independent of integration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMediaPreferences {
    #[serde(default = "default_subtitle_languages")]
    pub subtitle_languages: Vec<String>,
    #[serde(default = "default_subtitle_mode")]
    pub subtitle_mode: String,
    #[serde(default = "default_audio_languages")]
    pub audio_languages: Vec<String>,
    #[serde(default = "default_audio_mode")]
    pub audio_mode: String,
    #[serde(default = "default_video_quality")]
    pub video_quality: String,
    #[serde(default = "default_true")]
    pub thumbnail: bool,
    #[serde(default = "default_true")]
    pub automatic_subtitles: bool,
}

impl Default for UserMediaPreferences {
    fn default() -> Self {
        Self {
            subtitle_languages: default_subtitle_languages(),
            subtitle_mode: default_subtitle_mode(),
            audio_languages: default_audio_languages(),
            audio_mode: default_audio_mode(),
            video_quality: default_video_quality(),
            thumbnail: default_true(),
            automatic_subtitles: default_true(),
        }
    }
}

impl UserMediaPreferences {
    pub fn to_asset_policy(&self) -> AssetPolicy {
        let video = match self.video_quality.as_str() {
            "1080" | "720" => self.video_quality.clone(),
            "best_1080p" => "best_1080p".into(),
            _ => "best".into(),
        };
        AssetPolicy {
            video,
            thumbnail: self.thumbnail,
            channel_avatar: true,
            subtitles: self.subtitle_mode.clone(),
            subtitle_languages: self.subtitle_languages.clone(),
            automatic_subtitles: self.automatic_subtitles,
            audio_tracks: self.audio_mode.clone(),
            audio_languages: self.audio_languages.clone(),
        }
    }

    pub fn metadata_only_policy() -> AssetPolicy {
        AssetPolicy {
            video: "none".into(),
            thumbnail: true,
            channel_avatar: true,
            subtitles: "preferred".into(),
            ..AssetPolicy::default()
        }
    }
}
