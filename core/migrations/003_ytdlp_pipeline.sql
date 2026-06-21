-- Phase 2 pipeline: failures, subscriptions, entity relations

CREATE TABLE IF NOT EXISTS source_failure (
    id            TEXT PRIMARY KEY NOT NULL,
    job_id        TEXT,
    source        TEXT NOT NULL,
    kind          TEXT NOT NULL,
    external_id   TEXT NOT NULL,
    url           TEXT,
    stage         TEXT NOT NULL,
    error_kind    TEXT NOT NULL,
    message       TEXT NOT NULL,
    retryable     INTEGER NOT NULL DEFAULT 0,
    attempts      INTEGER NOT NULL DEFAULT 1,
    last_seen_at  TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_source_failure_lookup ON source_failure(source, kind, external_id);
CREATE INDEX IF NOT EXISTS idx_source_failure_job ON source_failure(job_id);

CREATE TABLE IF NOT EXISTS source_subscription (
    id                TEXT PRIMARY KEY NOT NULL,
    source            TEXT NOT NULL,
    kind              TEXT NOT NULL,
    url               TEXT NOT NULL,
    target_vault      TEXT NOT NULL,
    interval_minutes  INTEGER NOT NULL,
    next_run_at       TEXT NOT NULL,
    last_checked_at   TEXT,
    status            TEXT NOT NULL,
    created_at        TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_source_subscription_due ON source_subscription(status, next_run_at);

CREATE TABLE IF NOT EXISTS entity_relation (
    from_entity_id TEXT NOT NULL REFERENCES entity(id),
    to_entity_id   TEXT NOT NULL REFERENCES entity(id),
    relation       TEXT NOT NULL,
    PRIMARY KEY (from_entity_id, to_entity_id, relation)
);

CREATE INDEX IF NOT EXISTS idx_entity_relation_to ON entity_relation(to_entity_id, relation);
