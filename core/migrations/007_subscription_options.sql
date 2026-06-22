-- Subscription archive policy persisted as JSON

ALTER TABLE source_subscription ADD COLUMN options_json TEXT;
