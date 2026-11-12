-- Formation is provenance, not an assignment of the parent run to one new goal.
ALTER TABLE goals ADD COLUMN formed_in_run uuid;
ALTER TABLE goals ADD FOREIGN KEY(individual_id,formed_in_run) REFERENCES runs(individual_id,id);
-- Only trusted committed creation events qualify; owner-created goals stay NULL.
UPDATE goals g SET formed_in_run=r.id
FROM events e JOIN decision_commits c ON c.individual_id=e.individual_id
 AND c.decision_event_id::text=e.payload->>'decision_id'
JOIN runs r ON (r.individual_id,r.id)=(c.individual_id,c.run_id)
WHERE g.individual_id=e.individual_id AND e.kind='goal.changed'
 AND e.payload->>'goal_id'=g.id::text AND e.payload->>'from' IS NULL
 AND e.payload->>'to'='proposed';
CREATE INDEX goal_formation_run ON goals(individual_id,formed_in_run);
