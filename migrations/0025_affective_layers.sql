-- Finite appraisal parameters are engineering state, not measured experience.
ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN
 ('identity','event','memory','goal','run','relationship','action','entity','belief','autobiography','affective_state','affective_attitude'));
CREATE TABLE affective_appraisals (
 individual_id uuid NOT NULL,
 id uuid NOT NULL,
 goal_id uuid,
 subject_id uuid,
 valence double precision NOT NULL CHECK(valence BETWEEN -1 AND 1),
 arousal double precision NOT NULL CHECK(arousal BETWEEN 0 AND 1),
 goal_relevance double precision NOT NULL CHECK(goal_relevance BETWEEN 0 AND 1),
 expectation_gap double precision NOT NULL CHECK(expectation_gap BETWEEN -1 AND 1),
 controllability double precision NOT NULL CHECK(controllability BETWEEN 0 AND 1),
 uncertainty double precision NOT NULL CHECK(uncertainty BETWEEN 0 AND 1),
 brief_basis text NOT NULL CHECK(length(btrim(brief_basis))>0 AND length(brief_basis)<=1000),
 status text NOT NULL DEFAULT 'current' CHECK(status IN ('current','superseded')),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL,
 PRIMARY KEY(individual_id,id),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,goal_id) REFERENCES goals(individual_id,id),
 FOREIGN KEY(individual_id,subject_id) REFERENCES relationships(individual_id,subject_id),
 CHECK(expires_at>created_at)
);
CREATE UNIQUE INDEX current_affective_appraisal ON affective_appraisals(individual_id) WHERE status='current';
CREATE TABLE affective_attitudes (
 individual_id uuid NOT NULL,
 id uuid NOT NULL,
 topic text NOT NULL CHECK(topic ~ '^[a-z][a-z0-9_.-]{0,79}$'),
 subject_id uuid,
 stance text NOT NULL CHECK(length(btrim(stance))>0 AND length(stance)<=2000),
 brief_basis text NOT NULL CHECK(length(btrim(brief_basis))>0 AND length(brief_basis)<=1000),
 status text NOT NULL DEFAULT 'current' CHECK(status IN ('current','superseded')),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),
 supersedes uuid,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 superseded_at timestamptz,
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,supersedes),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,subject_id) REFERENCES relationships(individual_id,subject_id),
 FOREIGN KEY(individual_id,supersedes) REFERENCES affective_attitudes(individual_id,id),
 CHECK((status='superseded')=(superseded_at IS NOT NULL))
);
CREATE UNIQUE INDEX current_affective_attitude_topic ON affective_attitudes(individual_id,topic) WHERE status='current';
CREATE TABLE affective_sources (
 individual_id uuid NOT NULL,
 object_id uuid NOT NULL,
 event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,object_id,event_id),
 FOREIGN KEY(individual_id,object_id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
