-- Stance and its supporting/challenging evidence are not truth certificates.
ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN
 ('identity','event','memory','goal','run','relationship','action','entity','belief'));
CREATE TABLE beliefs (
 individual_id uuid NOT NULL,
 id uuid NOT NULL,
 statement text NOT NULL CHECK(length(statement) BETWEEN 1 AND 2000),
 scope text NOT NULL CHECK(length(scope) BETWEEN 1 AND 200),
 position text NOT NULL CHECK(position IN ('tentative','held','questioned')),
 brief_basis text NOT NULL CHECK(length(brief_basis) BETWEEN 1 AND 1000),
 status text NOT NULL DEFAULT 'current' CHECK(status IN ('current','superseded')),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),
 supersedes uuid,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 superseded_at timestamptz,
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,supersedes),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,supersedes) REFERENCES beliefs(individual_id,id),
 CHECK((status='superseded')=(superseded_at IS NOT NULL))
);
CREATE TABLE belief_evidence (
 individual_id uuid NOT NULL,
 belief_id uuid NOT NULL,
 event_id uuid NOT NULL,
 role text NOT NULL CHECK(role IN ('support','counter')),
 PRIMARY KEY(individual_id,belief_id,event_id,role),
 FOREIGN KEY(individual_id,belief_id) REFERENCES beliefs(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE INDEX current_beliefs ON beliefs(individual_id,created_at,id) WHERE status='current';
