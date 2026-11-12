-- Root-adopted M3 development persistence. Trial evidence is verified by the runtime.
-- This migration changes schema; deletion/export controllers must explicitly support it.
-- Trusted measurement/registration records have no model/HTTP insertion path.
ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN
 ('identity','event','memory','goal','run','relationship','action','entity','belief','autobiography','affective_state','affective_attitude','relationship_revision','interest','interest_revision','development_target','development','development_revision','development_verifier'));
CREATE TABLE development_targets (
 individual_id uuid NOT NULL, id uuid NOT NULL,
 target text NOT NULL CHECK(target IN ('persona.traits','persona.values','persona.expression','persona.cognitive_preferences','task_strategy')),
 scope_kind text CHECK(scope_kind IN ('goal','task')), scope_id uuid,
 version bigint NOT NULL DEFAULT 1 CHECK(version>0), value jsonb NOT NULL,
 current_revision_id uuid,
 PRIMARY KEY(individual_id,id),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,scope_id) REFERENCES objects(individual_id,id),
 CHECK((target='task_strategy' AND scope_kind IS NOT NULL AND scope_id IS NOT NULL) OR (target<>'task_strategy' AND scope_kind IS NULL AND scope_id IS NULL)),
 UNIQUE NULLS NOT DISTINCT(individual_id,target,scope_kind,scope_id)
);
CREATE TABLE developments (
 individual_id uuid NOT NULL, id uuid NOT NULL, target_id uuid NOT NULL,
 proposer_id uuid NOT NULL, draft jsonb NOT NULL CHECK(jsonb_typeof(draft)='object'),
 before_value jsonb NOT NULL, before_head uuid, prepared_at timestamptz NOT NULL,
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),
 status text NOT NULL CHECK(status IN ('proposed','trial','accepted','rejected','expired','reverted')),
 created_event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,id),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,target_id) REFERENCES development_targets(individual_id,id),
 FOREIGN KEY(individual_id,proposer_id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,created_event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE development_evidence (
 individual_id uuid NOT NULL, development_id uuid NOT NULL, event_id uuid NOT NULL,
 role text NOT NULL CHECK(role IN ('evidence','counterevidence')),
 PRIMARY KEY(individual_id,development_id,event_id),
 FOREIGN KEY(individual_id,development_id) REFERENCES developments(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE development_steps (
 individual_id uuid NOT NULL, development_id uuid NOT NULL, version bigint NOT NULL CHECK(version>0),
 event_id uuid NOT NULL, occurred_at timestamptz NOT NULL, command jsonb NOT NULL,
 status text NOT NULL CHECK(status IN ('proposed','trial','accepted','rejected','expired','reverted')),
 transition_record jsonb, validation_record jsonb, revision_id uuid,
 PRIMARY KEY(individual_id,development_id,version), UNIQUE(individual_id,event_id),
 FOREIGN KEY(individual_id,development_id) REFERENCES developments(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE development_revisions (
 individual_id uuid NOT NULL, id uuid NOT NULL, target_id uuid NOT NULL, development_id uuid NOT NULL,
 event_id uuid NOT NULL, action text NOT NULL CHECK(action IN ('accept','revert')),
 before_version bigint NOT NULL CHECK(before_version>0), after_version bigint NOT NULL CHECK(after_version=before_version+1),
 before_value jsonb NOT NULL, after_value jsonb NOT NULL, previous_revision_id uuid,
 PRIMARY KEY(individual_id,id), UNIQUE(individual_id,target_id,after_version),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,target_id) REFERENCES development_targets(individual_id,id),
 FOREIGN KEY(individual_id,development_id) REFERENCES developments(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,previous_revision_id) REFERENCES development_revisions(individual_id,id)
);
ALTER TABLE development_targets ADD FOREIGN KEY(individual_id,current_revision_id) REFERENCES development_revisions(individual_id,id);
ALTER TABLE developments ADD FOREIGN KEY(individual_id,before_head) REFERENCES development_revisions(individual_id,id);
ALTER TABLE development_steps ADD FOREIGN KEY(individual_id,revision_id) REFERENCES development_revisions(individual_id,id);
-- FK-backed provenance remains enforceable if a general derivation edge is absent.
CREATE TABLE development_dependencies (
 individual_id uuid NOT NULL, object_id uuid NOT NULL, source_id uuid NOT NULL,
 PRIMARY KEY(individual_id,object_id,source_id), CHECK(object_id<>source_id),
 FOREIGN KEY(individual_id,object_id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,source_id) REFERENCES objects(individual_id,id)
);
CREATE TABLE development_verifiers (
 individual_id uuid NOT NULL, id uuid NOT NULL, kind text NOT NULL CHECK(kind IN ('human','measurement')),
 registration_event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,id),
 FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
 FOREIGN KEY(individual_id,registration_event_id) REFERENCES events(individual_id,id)
);
-- A trusted observer/actual executor writes these with the corresponding event.
-- This command adapter cannot register verifiers, mint observations or self-certify.
CREATE TABLE development_observations (
 individual_id uuid NOT NULL, event_id uuid NOT NULL, verifier_id uuid NOT NULL,
 trial_event_id uuid, role text NOT NULL CHECK(role IN ('significant_event','scenario_result','counter_search')),
 case_id uuid, context text, outcome text CHECK(outcome IN ('passed','failed','unverified')),
 PRIMARY KEY(individual_id,event_id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,verifier_id) REFERENCES development_verifiers(individual_id,id),
 FOREIGN KEY(individual_id,trial_event_id) REFERENCES development_steps(individual_id,event_id),
 CHECK((role='significant_event' AND trial_event_id IS NULL) OR (role<>'significant_event' AND trial_event_id IS NOT NULL AND outcome IS NOT NULL))
);
CREATE TABLE development_review_records (
 individual_id uuid NOT NULL, event_id uuid NOT NULL, development_id uuid NOT NULL,
 trial_event_id uuid NOT NULL, verifier_id uuid NOT NULL, assessment jsonb NOT NULL,
 PRIMARY KEY(individual_id,event_id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,development_id) REFERENCES developments(individual_id,id),
 FOREIGN KEY(individual_id,trial_event_id) REFERENCES development_steps(individual_id,event_id),
 FOREIGN KEY(individual_id,verifier_id) REFERENCES development_verifiers(individual_id,id)
);
CREATE TABLE development_review_sources (
 individual_id uuid NOT NULL, review_event_id uuid NOT NULL, event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,review_event_id,event_id),
 FOREIGN KEY(individual_id,review_event_id) REFERENCES development_review_records(individual_id,event_id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE INDEX developments_listing ON developments(individual_id,prepared_at,id);
