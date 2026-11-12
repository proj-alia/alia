-- Durable, content-free accounting survives removal of message content.
CREATE TABLE contact_windows (
    id uuid PRIMARY KEY,
    individual_id uuid NOT NULL REFERENCES individuals(id),
    timezone text NOT NULL,
    starts_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL CHECK(ends_at>starts_at),
    delivered_count bigint NOT NULL DEFAULT 0 CHECK(delivered_count>=0),
    last_delivered_at timestamptz,
    UNIQUE(individual_id,starts_at)
);
ALTER TABLE outgoing_messages ADD COLUMN contact_window_id uuid REFERENCES contact_windows(id);
ALTER TABLE outgoing_messages ADD COLUMN cancellation_reason text;
CREATE INDEX pending_contact ON outgoing_messages(individual_id,status) WHERE mode='background';
