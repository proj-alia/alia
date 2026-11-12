-- One dispatched opportunity assessment per imported observation.
-- Undispatched terminal runs may be requeued after the cooldown.
-- Repeated idle checks cannot spend more calls on the same unchanged source.
CREATE TABLE discovery_occurrences (
    individual_id uuid NOT NULL,
    source_event_id uuid NOT NULL,
    run_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(individual_id, source_event_id),
    FOREIGN KEY(individual_id, source_event_id) REFERENCES events(individual_id,id),
    FOREIGN KEY(individual_id, run_id) REFERENCES runs(individual_id,id)
);
CREATE INDEX discovery_cooldown ON discovery_occurrences(individual_id,created_at DESC);
