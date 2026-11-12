-- Imported text and notes are owned database artifacts, never arbitrary host paths.
CREATE TABLE documents (
    individual_id uuid NOT NULL,
    event_id uuid NOT NULL,
    title text NOT NULL,
    source_label text NOT NULL,
    body text NOT NULL CHECK(octet_length(body) BETWEEN 1 AND 1048576),
    PRIMARY KEY(individual_id,event_id),
    FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE notes (
    individual_id uuid NOT NULL,
    action_id uuid NOT NULL,
    title text NOT NULL,
    body text NOT NULL CHECK(octet_length(body) BETWEEN 1 AND 16384),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(individual_id,action_id),
    FOREIGN KEY(individual_id,action_id) REFERENCES actions(individual_id,id)
);
ALTER TABLE actions ADD COLUMN receipt jsonb;
