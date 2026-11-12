-- Reviewer opinions are evidence-linked, version-bound, and charged model calls.
CREATE TABLE goal_reviews (
    individual_id uuid NOT NULL,
    event_id uuid NOT NULL,
    goal_id uuid NOT NULL,
    goal_version bigint NOT NULL,
    privacy_epoch bigint NOT NULL,
    input_digest text NOT NULL CHECK(length(input_digest)=64),
    reservation_id uuid NOT NULL UNIQUE REFERENCES usage_reservations(id),
    assessment jsonb NOT NULL,
    state_version bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(individual_id,event_id),
    FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id),
    FOREIGN KEY(individual_id,goal_id) REFERENCES goals(individual_id,id),
    UNIQUE(individual_id,goal_id,goal_version,privacy_epoch,input_digest)
);
