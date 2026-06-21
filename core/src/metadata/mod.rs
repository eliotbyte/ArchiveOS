//! Shared metadata provenance helpers.

use std::collections::HashMap;

use archiveos_contract::MetadataEntry;

const PRECEDENCE: &[&str] = &[
    "user",
    "yt-dlp",
    "ffprobe",
    "archiveos",
    "inferred",
    "system",
    "source",
    "extracted",
];

pub fn precedence_rank(provenance: &str) -> i32 {
    PRECEDENCE
        .iter()
        .position(|p| *p == provenance)
        .map(|i| i as i32)
        .unwrap_or(99)
}

pub fn title_precedence_sql() -> &'static str {
    "CASE provenance
        WHEN 'user' THEN 0
        WHEN 'yt-dlp' THEN 1
        WHEN 'ffprobe' THEN 2
        WHEN 'archiveos' THEN 3
        WHEN 'inferred' THEN 4
        WHEN 'system' THEN 5
        WHEN 'source' THEN 6
        WHEN 'extracted' THEN 7
        ELSE 8 END"
}

pub fn hidden_entity_sql(entity_alias: &str) -> String {
    format!(
        "NOT EXISTS (
            SELECT 1 FROM metadata hidden
            WHERE hidden.entity_id = {entity_alias}.id
              AND hidden.key = 'visibility'
              AND hidden.value = 'hidden'
        )"
    )
}

pub fn flatten_metadata(entries: &[MetadataEntry]) -> HashMap<String, String> {
    let mut by_key: HashMap<&str, &MetadataEntry> = HashMap::new();
    for entry in entries {
        match by_key.get(entry.key.as_str()) {
            Some(existing) if precedence_rank(&existing.provenance) <= precedence_rank(&entry.provenance) => {}
            _ => {
                by_key.insert(&entry.key, entry);
            }
        }
    }
    by_key
        .into_values()
        .map(|e| (e.key.clone(), e.value.clone()))
        .collect()
}

pub fn best_value_for_key(entries: &[MetadataEntry], key: &str) -> Option<String> {
    entries
        .iter()
        .filter(|e| e.key == key)
        .min_by_key(|e| precedence_rank(&e.provenance))
        .map(|e| e.value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_metadata_prefers_user_over_ytdlp() {
        let entries = vec![
            MetadataEntry {
                key: "title".into(),
                value: "User Title".into(),
                provenance: "user".into(),
            },
            MetadataEntry {
                key: "title".into(),
                value: "YT Title".into(),
                provenance: "yt-dlp".into(),
            },
        ];
        let flat = flatten_metadata(&entries);
        assert_eq!(flat.get("title").map(String::as_str), Some("User Title"));
    }
}
