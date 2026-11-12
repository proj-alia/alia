-- A current relationship, short appraisal and persistent attitude are different.
ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN
 ('identity','event','memory','goal','run','relationship','action','entity','belief','autobiography','affective_state','affective_attitude','relationship_revision'));
CREATE TABLE relationship_revisions (
 individual_id uuid NOT NULL,
 id uuid NOT NULL,
 subject_id uuid NOT NULL,
 before_version bigint NOT NULL CHECK(before_version>0),
 after_version bigint NOT NULL CHECK(after_version=before_version+1),
 before_state jsonb NOT NULL CHECK(jsonb_typeof(before_state)='object'),
 after_state jsonb NOT NULL CHECK(jsonb_typeof(after_state)='object'),
 brief_basis text NOT NULL CHECK(length(btrim(brief_basis))>0 AND length(brief_basis)<=1000),
 previous_revision_id uuid,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,subject_id,after_version),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,subject_id) REFERENCES relationships(individual_id,subject_id),
 FOREIGN KEY(individual_id,previous_revision_id) REFERENCES relationship_revisions(individual_id,id)
);
CREATE TABLE relationship_revision_events (
 individual_id uuid NOT NULL,
 revision_id uuid NOT NULL,
 event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,revision_id,event_id),
 FOREIGN KEY(individual_id,revision_id) REFERENCES relationship_revisions(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE INDEX relationship_revision_history ON relationship_revisions(individual_id,subject_id,created_at,id);
