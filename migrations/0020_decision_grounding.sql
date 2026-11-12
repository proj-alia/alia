-- Internal assessments do not change persona or publish a rejected candidate.
CREATE TABLE decision_reviews (
    author_reservation_id uuid PRIMARY KEY REFERENCES usage_reservations(id),
    review_reservation_id uuid UNIQUE NOT NULL REFERENCES usage_reservations(id),
    individual_id uuid NOT NULL,
    run_id uuid NOT NULL,
    event_id uuid UNIQUE NOT NULL,
    lease_generation bigint NOT NULL,
    state_version bigint NOT NULL,
    privacy_epoch bigint NOT NULL,
    config_version bigint NOT NULL,
    draft_digest text NOT NULL,
    context_digest text NOT NULL,
    review_context_digest text NOT NULL,
    assessment jsonb NOT NULL,
    supported boolean NOT NULL,
    checker_version text NOT NULL,
    provider_mode text NOT NULL CHECK(provider_mode IN ('live','fixture')),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK(author_reservation_id<>review_reservation_id),
    FOREIGN KEY(individual_id,run_id) REFERENCES runs(individual_id,id),
    FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
