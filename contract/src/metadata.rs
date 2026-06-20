use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constants::LAYOUT_VERSION;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultMetadata {
    pub id: Uuid,
    pub layout_version: u32,
    pub created_at: DateTime<Utc>,
}

impl VaultMetadata {
    pub fn new(id: Uuid, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            layout_version: LAYOUT_VERSION,
            created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    #[test]
    fn metadata_serializes_to_expected_json_shape() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let created_at = Utc.with_ymd_and_hms(2026, 6, 20, 12, 0, 0).unwrap();
        let meta = VaultMetadata {
            id,
            layout_version: LAYOUT_VERSION,
            created_at,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"id\":\"550e8400-e29b-41d4-a716-446655440000\""));
        assert!(json.contains("\"layout_version\":1"));
        let back: VaultMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, id);
        assert_eq!(back.layout_version, 1);
    }
}
