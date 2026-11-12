ALTER TABLE goals ADD COLUMN completion jsonb;
ALTER TABLE notes ADD COLUMN citations jsonb NOT NULL DEFAULT '[]' CHECK(jsonb_typeof(citations)='array');
