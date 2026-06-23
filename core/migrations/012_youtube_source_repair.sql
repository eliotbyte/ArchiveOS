-- Repair vaults where migration 011 partially canonicalized youtube aliases.

DELETE FROM source_ref
WHERE source LIKE 'youtube%' AND source != 'youtube'
  AND EXISTS (
    SELECT 1 FROM source_ref canonical
    WHERE canonical.source = 'youtube'
      AND canonical.kind = source_ref.kind
      AND canonical.external_id = source_ref.external_id
  );

UPDATE source_ref
SET source = 'youtube'
WHERE source LIKE 'youtube%' AND source != 'youtube';
