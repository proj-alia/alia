ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check CHECK(kind IN ('identity','event','memory','goal','run','relationship','action'));
ALTER TABLE goals ADD COLUMN origin_ids jsonb NOT NULL DEFAULT '[]';
CREATE TABLE decision_commits (
    reservation_id uuid PRIMARY KEY REFERENCES usage_reservations(id),
    individual_id uuid NOT NULL,
    run_id uuid NOT NULL,
    lease_generation bigint NOT NULL,
    privacy_epoch bigint NOT NULL,
    proposal_digest text NOT NULL,
    decision_event_id uuid NOT NULL,
    receipt jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY(individual_id,run_id) REFERENCES runs(individual_id,id),
    FOREIGN KEY(individual_id,decision_event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE actions (
    individual_id uuid NOT NULL,
    id uuid NOT NULL,
    run_id uuid NOT NULL,
    decision_event_id uuid NOT NULL,
    sequence integer NOT NULL CHECK(sequence>0),
    capability_id text NOT NULL,
    capability_version text NOT NULL,
    arguments jsonb NOT NULL,
    status text NOT NULL DEFAULT 'planned' CHECK(status IN ('planned','executing','succeeded','failed','unknown','cancelled')),
    result_event_id uuid,
    lease_generation bigint,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(individual_id,id),
    UNIQUE(run_id,sequence),
    FOREIGN KEY(individual_id,id) REFERENCES objects(individual_id,id),
    FOREIGN KEY(individual_id,run_id) REFERENCES runs(individual_id,id),
    FOREIGN KEY(individual_id,decision_event_id) REFERENCES events(individual_id,id),
    FOREIGN KEY(individual_id,result_event_id) REFERENCES events(individual_id,id)
);
-- Drafting is not delivery. Background output waits for a separately authorized,
-- quota-checked local delivery transaction; no external communication is involved.
CREATE TABLE outgoing_messages (
    individual_id uuid NOT NULL,
    draft_event_id uuid NOT NULL,
    run_id uuid NOT NULL,
    mode text NOT NULL CHECK(mode IN ('foreground','background')),
    privacy_epoch bigint NOT NULL,
    status text NOT NULL CHECK(status IN ('pending','delivered','cancelled')),
    delivered_event_id uuid,
    delivered_at timestamptz,
    PRIMARY KEY(individual_id,draft_event_id),
    UNIQUE(run_id),
    FOREIGN KEY(individual_id,draft_event_id) REFERENCES events(individual_id,id),
    FOREIGN KEY(individual_id,delivered_event_id) REFERENCES events(individual_id,id),
    FOREIGN KEY(individual_id,run_id) REFERENCES runs(individual_id,id)
);
