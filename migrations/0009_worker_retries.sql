ALTER TABLE runs ADD COLUMN model_retries integer NOT NULL DEFAULT 0 CHECK(model_retries BETWEEN 0 AND 2);
ALTER TABLE runs ADD COLUMN repair_attempts integer NOT NULL DEFAULT 0 CHECK(repair_attempts BETWEEN 0 AND 1);
ALTER TABLE runs ADD COLUMN retry_after timestamptz;
ALTER TABLE goals ADD COLUMN resume_at timestamptz;
