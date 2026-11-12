-- Seed array positions name legacy interests only once. Future reads never
-- recreate identity from a mutable array position or a description.
ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN
 ('identity','event','memory','goal','run','relationship','action','entity','belief','autobiography','affective_state','affective_attitude','relationship_revision','interest','interest_revision'));
CREATE TABLE interests (
 individual_id uuid NOT NULL,
 id text NOT NULL CHECK(length(id) BETWEEN 1 AND 160),
 object_id uuid NOT NULL,
 description text NOT NULL CHECK(length(btrim(description))>0 AND length(description)<=2000),
 status text NOT NULL DEFAULT 'active' CHECK(status IN ('active','retired')),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),
 origin_kind text NOT NULL CHECK(origin_kind IN ('initial_seed','experience')),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,object_id),
 FOREIGN KEY(individual_id,object_id) REFERENCES objects(individual_id,id)
);
CREATE TABLE interest_sources (
 individual_id uuid NOT NULL,
 interest_id text NOT NULL,
 event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,interest_id,event_id),
 FOREIGN KEY(individual_id,interest_id) REFERENCES interests(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE interest_revisions (
 individual_id uuid NOT NULL,
 id uuid NOT NULL,
 interest_id text NOT NULL,
 before_version bigint NOT NULL CHECK(before_version>=0),
 after_version bigint NOT NULL CHECK(after_version=before_version+1),
 before_description text,
 before_status text CHECK(before_status IN ('active','retired')),
 after_description text NOT NULL CHECK(length(btrim(after_description))>0 AND length(after_description)<=2000),
 after_status text NOT NULL CHECK(after_status IN ('active','retired')),
 brief_basis text NOT NULL CHECK(length(btrim(brief_basis))>0 AND length(brief_basis)<=1000),
 event_id uuid NOT NULL,
 previous_revision_id uuid,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,interest_id,after_version),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,interest_id) REFERENCES interests(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,previous_revision_id) REFERENCES interest_revisions(individual_id,id)
);

CREATE TEMP TABLE alia_interest_seed ON COMMIT DROP AS
 SELECT i.id AS individual_id,format('interest:%s:%s',i.id,(s.n-1)) AS id,
        gen_random_uuid() AS object_id,s.description,b.id AS event_id
 FROM individuals i
 CROSS JOIN LATERAL jsonb_array_elements_text(i.seed->'personality'->'initial_interests')
   WITH ORDINALITY s(description,n)
 JOIN LATERAL (SELECT e.id FROM events e WHERE e.individual_id=i.id
   AND e.kind='individual.created' ORDER BY e.sequence LIMIT 1) b ON true;
INSERT INTO objects(individual_id,id,kind)
 SELECT individual_id,object_id,'interest' FROM alia_interest_seed;
INSERT INTO interests(individual_id,id,object_id,description,origin_kind)
 SELECT individual_id,id,object_id,description,'initial_seed' FROM alia_interest_seed;
INSERT INTO interest_sources(individual_id,interest_id,event_id)
 SELECT individual_id,id,event_id FROM alia_interest_seed;
INSERT INTO derivation_edges(individual_id,source_id,target_id)
 SELECT individual_id,event_id,object_id FROM alia_interest_seed ON CONFLICT DO NOTHING;
