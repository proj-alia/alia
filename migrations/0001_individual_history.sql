-- One installation, one identity. A terminated identity cannot be replaced implicitly.
CREATE TABLE individuals (
    id uuid PRIMARY KEY,
    singleton boolean NOT NULL DEFAULT true UNIQUE CHECK (singleton),
    created_at timestamptz NOT NULL DEFAULT now(),
    origin jsonb NOT NULL,
    seed jsonb NOT NULL,
    seed_digest text NOT NULL CHECK (length(seed_digest) = 64),
    persona jsonb NOT NULL,
    runtime_config jsonb NOT NULL,
    lifecycle text NOT NULL DEFAULT 'active' CHECK (lifecycle IN ('active','paused','archived','terminated')),
    state_version bigint NOT NULL DEFAULT 1 CHECK (state_version > 0),
    privacy_epoch bigint NOT NULL DEFAULT 0 CHECK (privacy_epoch >= 0),
    config_version bigint NOT NULL DEFAULT 1 CHECK (config_version > 0)
);

-- Every derived object has an ownership/provenance identity, independent of its content table.
CREATE TABLE objects (
    individual_id uuid NOT NULL REFERENCES individuals(id),
    id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('identity','event','memory','goal','run','relationship')),
    blocked boolean NOT NULL DEFAULT false,
    PRIMARY KEY (individual_id, id)
);

CREATE TABLE events (
    individual_id uuid NOT NULL,
    id uuid NOT NULL,
    sequence bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
    kind text NOT NULL,
    source_kind text NOT NULL CHECK (source_kind IN ('direct_observation','reported_claim','external_document','inference','generated','fictional_background')),
    actor_id text NOT NULL,
    occurred_at timestamptz NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT now(),
    payload jsonb NOT NULL,
    correlation_id uuid NOT NULL,
    causation_id uuid,
    idempotency_key text,
    PRIMARY KEY (individual_id, id),
    FOREIGN KEY (individual_id, id) REFERENCES objects(individual_id, id),
    FOREIGN KEY (individual_id, causation_id) REFERENCES events(individual_id, id)
);
CREATE INDEX events_chronology ON events(individual_id, sequence);

CREATE TABLE relationships (
    individual_id uuid NOT NULL,
    subject_id uuid NOT NULL,
    subject_key text NOT NULL,
    stage text NOT NULL DEFAULT 'unfamiliar',
    expectations jsonb NOT NULL DEFAULT '[]',
    boundaries jsonb NOT NULL DEFAULT '[]',
    commitments jsonb NOT NULL DEFAULT '[]',
    unresolved jsonb NOT NULL DEFAULT '[]',
    version bigint NOT NULL DEFAULT 1,
    PRIMARY KEY (individual_id, subject_id),
    UNIQUE (individual_id, subject_key),
    FOREIGN KEY (individual_id, subject_id) REFERENCES objects(individual_id, id)
);
CREATE TABLE relationship_events (
    individual_id uuid NOT NULL,
    subject_id uuid NOT NULL,
    event_id uuid NOT NULL,
    PRIMARY KEY (individual_id, subject_id, event_id),
    FOREIGN KEY (individual_id, subject_id) REFERENCES relationships(individual_id, subject_id),
    FOREIGN KEY (individual_id, event_id) REFERENCES events(individual_id, id)
);

CREATE TABLE memories (
    individual_id uuid NOT NULL,
    id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('working','episodic','semantic','procedural','autobiographical','relational','affective','prospective','meta')),
    content text NOT NULL CHECK (length(content) BETWEEN 1 AND 2000),
    scope text NOT NULL CHECK (length(scope) BETWEEN 1 AND 200),
    validity text NOT NULL DEFAULT 'active' CHECK (validity IN ('active','disputed','superseded','stale')),
    version bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    valid_from timestamptz NOT NULL DEFAULT now(),
    valid_until timestamptz,
    supersedes uuid,
    embedding_version text,
    PRIMARY KEY (individual_id, id),
    FOREIGN KEY (individual_id, id) REFERENCES objects(individual_id, id),
    FOREIGN KEY (individual_id, supersedes) REFERENCES memories(individual_id, id)
);
CREATE TABLE memory_sources (
    individual_id uuid NOT NULL,
    memory_id uuid NOT NULL,
    event_id uuid NOT NULL,
    PRIMARY KEY (individual_id, memory_id, event_id),
    FOREIGN KEY (individual_id, memory_id) REFERENCES memories(individual_id, id),
    FOREIGN KEY (individual_id, event_id) REFERENCES events(individual_id, id)
);

CREATE TABLE goals (
    individual_id uuid NOT NULL,
    id uuid NOT NULL,
    origin_type text NOT NULL CHECK (origin_type IN ('event','interest')),
    description text NOT NULL,
    success_criterion text NOT NULL,
    next_step text NOT NULL,
    stop_condition text NOT NULL,
    priority integer NOT NULL DEFAULT 50 CHECK (priority BETWEEN 0 AND 100),
    due_at timestamptz,
    status text NOT NULL DEFAULT 'proposed' CHECK (status IN ('proposed','active','waiting','completed','abandoned')),
    waiting_reason text,
    version bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (individual_id, id),
    FOREIGN KEY (individual_id, id) REFERENCES objects(individual_id, id)
);

CREATE TABLE runs (
    individual_id uuid NOT NULL,
    id uuid NOT NULL,
    trigger_event_id uuid NOT NULL,
    goal_id uuid,
    mode text NOT NULL CHECK (mode IN ('foreground','background')),
    state_version bigint NOT NULL,
    privacy_epoch bigint NOT NULL,
    config_version bigint NOT NULL,
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','running','committed','failed','cancelled','stale')),
    input_manifest uuid[] NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (individual_id, id),
    FOREIGN KEY (individual_id, id) REFERENCES objects(individual_id, id),
    FOREIGN KEY (individual_id, trigger_event_id) REFERENCES events(individual_id, id),
    FOREIGN KEY (individual_id, goal_id) REFERENCES goals(individual_id, id)
);

CREATE TABLE derivation_edges (
    individual_id uuid NOT NULL,
    source_id uuid NOT NULL,
    target_id uuid NOT NULL,
    PRIMARY KEY (individual_id, source_id, target_id),
    FOREIGN KEY (individual_id, source_id) REFERENCES objects(individual_id, id),
    FOREIGN KEY (individual_id, target_id) REFERENCES objects(individual_id, id),
    CHECK (source_id <> target_id)
);
CREATE INDEX derivation_targets ON derivation_edges(individual_id, target_id);

CREATE TABLE idempotency_records (
    actor_id text NOT NULL,
    route text NOT NULL,
    key text NOT NULL CHECK (length(key) BETWEEN 1 AND 128),
    body_digest text NOT NULL CHECK (length(body_digest) = 64),
    response jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (actor_id, route, key)
);

CREATE TABLE outbox (
    sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    individual_id uuid NOT NULL REFERENCES individuals(id),
    event_type text NOT NULL,
    resource_id uuid NOT NULL,
    version bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
