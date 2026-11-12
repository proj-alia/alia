-- API versions include scheduling metadata. Review revisions change only with
-- goal intent/state transitions, so a cooldown tick cannot invalidate a result.
ALTER TABLE goals ADD COLUMN review_revision bigint NOT NULL DEFAULT 1 CHECK(review_revision>0);
ALTER TABLE goal_reviews RENAME COLUMN goal_version TO goal_revision;
