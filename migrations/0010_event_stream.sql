-- The watermark survives pruning, including an entirely empty retained outbox.
CREATE TABLE event_stream_watermarks (
    individual_id uuid PRIMARY KEY REFERENCES individuals(id),
    discarded_through bigint NOT NULL DEFAULT 0 CHECK (discarded_through >= 0)
);
CREATE INDEX outbox_individual_sequence ON outbox(individual_id, sequence);
