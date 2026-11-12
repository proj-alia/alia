ALTER TABLE goals ADD COLUMN no_progress_cycles integer NOT NULL DEFAULT 0 CHECK(no_progress_cycles>=0);
ALTER TABLE goals ADD COLUMN waiting_since_sequence bigint;
ALTER TABLE runs ADD COLUMN goal_due_at timestamptz;
CREATE UNIQUE INDEX one_open_goal_run ON runs(individual_id,goal_id) WHERE goal_id IS NOT NULL AND status IN ('queued','running');
CREATE UNIQUE INDEX one_goal_due_occurrence ON runs(individual_id,goal_id,goal_due_at) WHERE goal_due_at IS NOT NULL;
CREATE TABLE goal_cycles (
    individual_id uuid NOT NULL,
    run_id uuid NOT NULL,
    goal_id uuid NOT NULL,
    outcome text NOT NULL,
    novel_results integer NOT NULL CHECK(novel_results>=0),
    recorded_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(individual_id,run_id),
    FOREIGN KEY(individual_id,run_id) REFERENCES runs(individual_id,id),
    FOREIGN KEY(individual_id,goal_id) REFERENCES goals(individual_id,id)
);
CREATE TABLE goal_progress (
    individual_id uuid NOT NULL,
    goal_id uuid NOT NULL,
    content_digest text NOT NULL CHECK(length(content_digest)=64),
    event_id uuid NOT NULL,
    PRIMARY KEY(individual_id,goal_id,content_digest),
    FOREIGN KEY(individual_id,goal_id) REFERENCES goals(individual_id,id),
    FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
