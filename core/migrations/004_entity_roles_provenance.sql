-- Backfill supporting/hidden role metadata for existing thumbnail entities.

INSERT INTO metadata (entity_id, key, value, provenance)
SELECT sr.entity_id, 'entity_role', 'supporting', 'archiveos'
FROM source_ref sr
WHERE sr.kind = 'thumbnail'
  AND NOT EXISTS (
    SELECT 1 FROM metadata m
    WHERE m.entity_id = sr.entity_id
      AND m.key = 'entity_role'
      AND m.provenance = 'archiveos'
  );

INSERT INTO metadata (entity_id, key, value, provenance)
SELECT sr.entity_id, 'asset_role', 'thumbnail', 'archiveos'
FROM source_ref sr
WHERE sr.kind = 'thumbnail'
  AND NOT EXISTS (
    SELECT 1 FROM metadata m
    WHERE m.entity_id = sr.entity_id
      AND m.key = 'asset_role'
      AND m.provenance = 'archiveos'
  );

INSERT INTO metadata (entity_id, key, value, provenance)
SELECT sr.entity_id, 'visibility', 'hidden', 'archiveos'
FROM source_ref sr
WHERE sr.kind = 'thumbnail'
  AND NOT EXISTS (
    SELECT 1 FROM metadata m
    WHERE m.entity_id = sr.entity_id
      AND m.key = 'visibility'
      AND m.provenance = 'archiveos'
  );

INSERT INTO metadata (entity_id, key, value, provenance)
SELECT er.from_entity_id, 'thumbnail_external_id', sr.external_id, 'archiveos'
FROM entity_relation er
JOIN source_ref sr ON sr.entity_id = er.to_entity_id AND sr.kind = 'thumbnail'
WHERE er.relation = 'thumbnail'
  AND NOT EXISTS (
    SELECT 1 FROM metadata m
    WHERE m.entity_id = er.from_entity_id
      AND m.key = 'thumbnail_external_id'
      AND m.provenance = 'archiveos'
  );
