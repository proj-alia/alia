ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN ('identity','event','memory','goal','run','relationship','action','entity'));
CREATE TABLE person_entities (
 individual_id uuid NOT NULL,
 id uuid NOT NULL,
 external_key text NOT NULL CHECK(length(external_key) BETWEEN 1 AND 120),
 label text NOT NULL,
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,external_key),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id)
);
CREATE TABLE person_aliases (
 individual_id uuid NOT NULL,
 entity_id uuid NOT NULL,
 surface text NOT NULL CHECK(length(surface) BETWEEN 1 AND 80),
 source_event_id uuid NOT NULL,
 assertion_event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,entity_id,surface),
 FOREIGN KEY(individual_id,entity_id) REFERENCES person_entities(individual_id,id),
 FOREIGN KEY(individual_id,source_event_id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,assertion_event_id) REFERENCES events(individual_id,id)
);
CREATE INDEX person_alias_resolution ON person_aliases(individual_id,surface);
CREATE TABLE memory_person_links (
 individual_id uuid NOT NULL,
 memory_id uuid NOT NULL,
 memory_version bigint NOT NULL,
 content_digest text NOT NULL,
 entity_id uuid NOT NULL,
 role text NOT NULL CHECK(role IN ('subject','mentioned')),
 source_event_id uuid NOT NULL,
 assertion_event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,memory_id,entity_id,role),
 FOREIGN KEY(individual_id,memory_id) REFERENCES memories(individual_id,id),
 FOREIGN KEY(individual_id,entity_id) REFERENCES person_entities(individual_id,id),
 FOREIGN KEY(individual_id,source_event_id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,assertion_event_id) REFERENCES events(individual_id,id)
);
