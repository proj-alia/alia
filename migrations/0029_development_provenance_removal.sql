-- Individual development: provenance removal is independent of lifecycle status.
-- Privacy invalidation is orthogonal to the six development lifecycle states.
ALTER TABLE development_targets ADD COLUMN provenance_removed boolean NOT NULL DEFAULT false;
ALTER TABLE developments ADD COLUMN provenance_removed boolean NOT NULL DEFAULT false;
ALTER TABLE development_revisions ADD COLUMN provenance_removed boolean NOT NULL DEFAULT false;
DO $$
DECLARE constraint_name text;
BEGIN
 SELECT c.conname INTO STRICT constraint_name FROM pg_constraint c
 WHERE c.conrelid='development_targets'::regclass AND c.contype='u'
 AND c.conkey=ARRAY[
 (SELECT attnum FROM pg_attribute WHERE attrelid=c.conrelid AND attname='individual_id'),
 (SELECT attnum FROM pg_attribute WHERE attrelid=c.conrelid AND attname='target'),
 (SELECT attnum FROM pg_attribute WHERE attrelid=c.conrelid AND attname='scope_kind'),
 (SELECT attnum FROM pg_attribute WHERE attrelid=c.conrelid AND attname='scope_id')
 ]::smallint[];
 EXECUTE format('ALTER TABLE development_targets DROP CONSTRAINT %I',constraint_name);
END $$;
CREATE UNIQUE INDEX development_targets_live_scope ON development_targets
 (individual_id,target,scope_kind,scope_id) NULLS NOT DISTINCT WHERE NOT provenance_removed;
