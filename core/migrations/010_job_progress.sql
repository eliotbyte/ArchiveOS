-- Job progress reporting and parent/child hierarchy

ALTER TABLE job ADD COLUMN progress_json TEXT;
ALTER TABLE job ADD COLUMN parent_job_id TEXT REFERENCES job(id);

CREATE INDEX IF NOT EXISTS idx_job_parent ON job(parent_job_id);
