-- Logical entities can have many physical assets/renditions.

CREATE TABLE IF NOT EXISTS entity_asset (
    id               TEXT PRIMARY KEY NOT NULL,
    entity_id        TEXT NOT NULL REFERENCES entity(id),
    role             TEXT NOT NULL,
    kind             TEXT NOT NULL,
    content_hash     TEXT,
    mime             TEXT,
    size             INTEGER NOT NULL DEFAULT 0,
    ext              TEXT,
    status           TEXT NOT NULL,
    storage_strategy TEXT NOT NULL,
    path             TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT
);

CREATE INDEX IF NOT EXISTS idx_entity_asset_entity_id ON entity_asset(entity_id);
CREATE INDEX IF NOT EXISTS idx_entity_asset_content_hash ON entity_asset(content_hash);
CREATE INDEX IF NOT EXISTS idx_entity_asset_status ON entity_asset(status);

INSERT INTO entity_asset (
    id, entity_id, role, kind, content_hash, mime, size, ext, status,
    storage_strategy, path, created_at, updated_at, deleted_at
)
SELECT
    lower(hex(randomblob(4))) || '-' ||
    lower(hex(randomblob(2))) || '-' ||
    '4' || substr(lower(hex(randomblob(2))), 2) || '-' ||
    substr('89ab', abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))), 2) || '-' ||
    lower(hex(randomblob(6))),
    e.id,
    CASE WHEN sr.kind = 'thumbnail' THEN 'supporting' ELSE 'primary' END,
    COALESCE(sr.kind, 'file'),
    e.content_hash,
    e.mime,
    COALESCE(e.size, 0),
    NULL,
    CASE WHEN e.status = 'present' THEN 'present' ELSE e.status END,
    'managed',
    NULL,
    e.added_at,
    e.added_at,
    NULL
FROM entity e
LEFT JOIN source_ref sr ON sr.entity_id = e.id
WHERE e.content_hash IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM entity_asset a
    WHERE a.entity_id = e.id
      AND a.content_hash = e.content_hash
  );

ALTER TABLE collection_member ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE collection_member ADD COLUMN first_seen_at TEXT;
ALTER TABLE collection_member ADD COLUMN last_seen_at TEXT;
ALTER TABLE collection_member ADD COLUMN removed_at TEXT;

UPDATE collection_member
SET first_seen_at = COALESCE(first_seen_at, datetime('now')),
    last_seen_at = COALESCE(last_seen_at, datetime('now'))
WHERE first_seen_at IS NULL OR last_seen_at IS NULL;
