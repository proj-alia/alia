-- An interpretation of personal history cannot replace the events it cites.
ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN
 ('identity','event','memory','goal','run','relationship','action','entity','belief','autobiography'));

CREATE TABLE autobiographies (
 individual_id uuid NOT NULL,
 id uuid NOT NULL,
 topic text NOT NULL CHECK(topic ~ '^[a-z][a-z0-9_.-]{0,79}$'),
 title text NOT NULL CHECK(length(btrim(title))>0 AND length(title)<=160),
 interpretation text NOT NULL CHECK(length(btrim(interpretation))>0 AND length(interpretation)<=4000),
 status text NOT NULL DEFAULT 'current' CHECK(status IN ('current','superseded')),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),
 supersedes uuid,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 superseded_at timestamptz,
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,supersedes),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,supersedes) REFERENCES autobiographies(individual_id,id),
 CHECK((status='superseded')=(superseded_at IS NOT NULL))
);
CREATE UNIQUE INDEX current_autobiography_topic ON autobiographies(individual_id,topic)
 WHERE status='current';
CREATE INDEX autobiography_history ON autobiographies(individual_id,created_at,id);

CREATE TABLE autobiography_events (
 individual_id uuid NOT NULL,
 autobiography_id uuid NOT NULL,
 event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,autobiography_id,event_id),
 FOREIGN KEY(individual_id,autobiography_id) REFERENCES autobiographies(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE autobiography_beliefs (
 individual_id uuid NOT NULL,
 autobiography_id uuid NOT NULL,
 belief_id uuid NOT NULL,
 PRIMARY KEY(individual_id,autobiography_id,belief_id),
 FOREIGN KEY(individual_id,autobiography_id) REFERENCES autobiographies(individual_id,id),
 FOREIGN KEY(individual_id,belief_id) REFERENCES beliefs(individual_id,id)
);
