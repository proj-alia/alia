-- Lossless legacy projection: mixed IDs use the existing disjoint event UUID
-- and runtime-generated interest:<individual UUID>:<index> namespaces.
-- No historical row or identity is rewritten.
ALTER TABLE goals DROP CONSTRAINT goals_origin_type_check;
ALTER TABLE goals ADD CONSTRAINT goals_origin_type_check
    CHECK (origin_type IN ('event','interest','mixed'));
