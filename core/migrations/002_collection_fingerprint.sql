ALTER TABLE collection ADD COLUMN content_fingerprint TEXT;
CREATE INDEX idx_collection_content_fingerprint ON collection(content_fingerprint);
