-- Asset-level metadata for subtitles, audio tracks, and other renditions.

CREATE TABLE IF NOT EXISTS entity_asset_metadata (
    asset_id   TEXT NOT NULL REFERENCES entity_asset(id),
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    provenance TEXT NOT NULL,
    PRIMARY KEY (asset_id, key, provenance)
);

CREATE INDEX IF NOT EXISTS idx_entity_asset_metadata_asset_id
    ON entity_asset_metadata(asset_id);
