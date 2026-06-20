-- Phase 1 schema (PLAN.md)

CREATE TABLE entity (
    id            TEXT PRIMARY KEY NOT NULL,
    content_hash  TEXT,
    mime          TEXT,
    size          INTEGER,
    status        TEXT NOT NULL,
    added_at      TEXT NOT NULL,
    created_at    TEXT
);

CREATE TABLE source_ref (
    id           TEXT PRIMARY KEY NOT NULL,
    entity_id    TEXT NOT NULL REFERENCES entity(id),
    source       TEXT NOT NULL,
    kind         TEXT NOT NULL,
    external_id  TEXT NOT NULL,
    url          TEXT,
    status       TEXT NOT NULL,
    UNIQUE(source, kind, external_id)
);

CREATE TABLE metadata (
    entity_id    TEXT NOT NULL REFERENCES entity(id),
    key          TEXT NOT NULL,
    value        TEXT NOT NULL,
    provenance   TEXT NOT NULL,
    PRIMARY KEY (entity_id, key, provenance)
);

CREATE TABLE tag (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE entity_tag (
    entity_id TEXT NOT NULL REFERENCES entity(id),
    tag_id    INTEGER NOT NULL REFERENCES tag(id),
    PRIMARY KEY (entity_id, tag_id)
);

CREATE TABLE collection (
    id    TEXT PRIMARY KEY NOT NULL,
    type  TEXT NOT NULL,
    title TEXT NOT NULL
);

CREATE TABLE collection_member (
    collection_id TEXT NOT NULL REFERENCES collection(id),
    entity_id     TEXT NOT NULL REFERENCES entity(id),
    position      INTEGER NOT NULL,
    PRIMARY KEY (collection_id, entity_id)
);

CREATE TABLE job (
    id            TEXT PRIMARY KEY NOT NULL,
    type          TEXT NOT NULL,
    target_vault  TEXT NOT NULL,
    input         TEXT NOT NULL,
    status        TEXT NOT NULL,
    lease_until   TEXT,
    attempts      INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_entity_content_hash ON entity(content_hash);
CREATE INDEX idx_source_ref_entity_id ON source_ref(entity_id);
CREATE INDEX idx_job_status ON job(status);
