-- Progress survives removal of the target content. No plaintext or credentials.
CREATE TABLE privacy_operations (
 individual_id uuid NOT NULL REFERENCES individuals(id),
 id uuid NOT NULL,
 target_id uuid NOT NULL,
 expected_memory_version bigint NOT NULL CHECK(expected_memory_version>0),
 idempotency_key_digest text NOT NULL CHECK(idempotency_key_digest ~ '^[0-9a-f]{64}$'),
 request_digest text NOT NULL CHECK(request_digest ~ '^[0-9a-f]{64}$'),
 journal_sequence bigint NOT NULL CHECK(journal_sequence>0),
 journal_digest text NOT NULL CHECK(journal_digest ~ '^[0-9a-f]{64}$'),
 root_event_ids uuid[] NOT NULL CHECK(cardinality(root_event_ids) BETWEEN 1 AND 256),
 object_ids uuid[] NOT NULL CHECK(cardinality(object_ids) BETWEEN 1 AND 10000),
 status text NOT NULL DEFAULT 'blocked' CHECK(status IN ('blocked','purging','retry_pending','completed')),
 state_version bigint NOT NULL CHECK(state_version>0),
 privacy_epoch bigint NOT NULL CHECK(privacy_epoch>0),
 last_error text CHECK(last_error ~ '^[a-z][a-z0-9_]{0,79}$'),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(individual_id,id),
 UNIQUE(individual_id,idempotency_key_digest),
 UNIQUE(individual_id,journal_sequence),
 CHECK(status<>'completed' OR last_error IS NULL)
);
