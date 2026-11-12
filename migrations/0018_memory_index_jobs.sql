-- Derived local computation only; these are not goals or personal experiences.
CREATE TABLE memory_index_jobs (
 individual_id uuid NOT NULL,
 memory_id uuid NOT NULL,
 profile text NOT NULL,
 memory_version bigint NOT NULL,
 privacy_epoch bigint NOT NULL,
 content_digest text NOT NULL,
 status text NOT NULL CHECK(status IN ('queued','running','ready','failed','cancelled')),
 attempts integer NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 3),
 generation bigint NOT NULL DEFAULT 0,
 lease_owner uuid,
 lease_expires_at timestamptz,
 available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 error_code text CHECK(error_code IN ('encoding_failed','encoding_timeout','lease_expired','source_changed','invalid_encoding')),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(individual_id,memory_id,profile),
 FOREIGN KEY(individual_id,memory_id) REFERENCES memories(individual_id,id) ON DELETE CASCADE,
 CHECK((status='running')=(lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL)),
 CHECK(status='running' OR (lease_owner IS NULL AND lease_expires_at IS NULL))
);
CREATE INDEX memory_index_jobs_pending ON memory_index_jobs(available_at,memory_id) WHERE status IN ('queued','running');
