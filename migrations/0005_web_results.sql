-- Preserve an immutable result identity when a network receipt is retried.
ALTER TABLE actions ADD COLUMN result_digest text CHECK(result_digest IS NULL OR length(result_digest)=64);
ALTER TABLE actions ADD COLUMN execution_mode text CHECK(execution_mode IS NULL OR execution_mode IN ('live','fixture'));
