-- Historical rows remain NULL; do not reconstruct unobserved start times.
ALTER TABLE actions ADD COLUMN started_at timestamptz;
ALTER TABLE actions ADD COLUMN finished_at timestamptz;
