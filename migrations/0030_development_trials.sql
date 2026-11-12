-- Trusted deterministic measurements. All work identities are actual events;
-- expected results stay in private rows, never in normal event payloads.
CREATE TABLE development_measurement_programs (
 individual_id uuid NOT NULL, verifier_id uuid NOT NULL,
 program text NOT NULL CHECK(program='json_comparison_v1'),
 version text NOT NULL CHECK(version='alia-json-comparison/1'),
 policy jsonb NOT NULL CHECK(jsonb_typeof(policy)='object'),
 policy_digest text NOT NULL CHECK(policy_digest ~ '^[0-9a-f]{64}$'),
 registration_event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,verifier_id), UNIQUE(individual_id,program,version),
 FOREIGN KEY(individual_id,verifier_id) REFERENCES development_verifiers(individual_id,id),
 FOREIGN KEY(individual_id,registration_event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE development_trial_plans (
 individual_id uuid NOT NULL, id uuid NOT NULL, development_id uuid NOT NULL,
 development_version bigint NOT NULL CHECK(development_version>0), trial_event_id uuid NOT NULL,
 evaluator_id uuid NOT NULL, plan jsonb NOT NULL CHECK(jsonb_typeof(plan)='object'),
 input_manifest uuid[] NOT NULL, cases_digest text NOT NULL CHECK(cases_digest ~ '^[0-9a-f]{64}$'),
 privacy_epoch bigint NOT NULL, config_version bigint NOT NULL,
 deadline timestamptz NOT NULL, status text NOT NULL DEFAULT 'active'
 CHECK(status IN ('active','reviewed','expired','stale')),
 review_event_id uuid, review_development_version bigint,
 review_outcome text CHECK(review_outcome IN ('passed','failed','unverified')),
 PRIMARY KEY(individual_id,id), UNIQUE(individual_id,development_id),
 FOREIGN KEY(individual_id,id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,development_id) REFERENCES developments(individual_id,id),
 FOREIGN KEY(individual_id,trial_event_id) REFERENCES development_steps(individual_id,event_id),
 FOREIGN KEY(individual_id,evaluator_id) REFERENCES development_measurement_programs(individual_id,verifier_id),
 FOREIGN KEY(individual_id,review_event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE development_trial_cases (
 individual_id uuid NOT NULL, id uuid NOT NULL, plan_id uuid NOT NULL,
 ordinal smallint NOT NULL CHECK(ordinal BETWEEN 0 AND 7),
 case_data jsonb NOT NULL CHECK(jsonb_typeof(case_data)='object'),
 evaluation_run_id uuid, lease_generation bigint,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','queued','claimed','finished','unknown','not_run')),
 observation_event_id uuid,
 PRIMARY KEY(individual_id,id), UNIQUE(individual_id,plan_id,ordinal),
 UNIQUE(individual_id,plan_id,id), UNIQUE(individual_id,evaluation_run_id),
 FOREIGN KEY(individual_id,id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,plan_id) REFERENCES development_trial_plans(individual_id,id),
 FOREIGN KEY(individual_id,evaluation_run_id) REFERENCES runs(individual_id,id),
 FOREIGN KEY(individual_id,observation_event_id) REFERENCES events(individual_id,id)
);
CREATE TABLE development_trial_attempts (
 individual_id uuid NOT NULL, id uuid NOT NULL, plan_id uuid NOT NULL, case_id uuid NOT NULL,
 side text NOT NULL CHECK(side IN ('baseline','trial')),
 evaluation_run_id uuid, lease_generation bigint,
 state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','reserved','dispatched','recorded','unknown','not_run')),
 context_json jsonb, request_digest text, request_sources uuid[] NOT NULL DEFAULT '{}',
 input_bound bigint CHECK(input_bound>0),
 output_cap integer CHECK(output_cap BETWEEN 1 AND 8192),
 reservation_id uuid UNIQUE REFERENCES usage_reservations(id),
 dispatch_mode text CHECK(dispatch_mode IN ('fixture','live')), attested_at timestamptz,
 result jsonb, result_event_id uuid, outcome text NOT NULL DEFAULT 'unverified'
 CHECK(outcome IN ('passed','failed','unverified')),
 PRIMARY KEY(individual_id,id), UNIQUE(individual_id,case_id,side),
 FOREIGN KEY(individual_id,id) REFERENCES events(individual_id,id),
 FOREIGN KEY(individual_id,plan_id,case_id) REFERENCES development_trial_cases(individual_id,plan_id,id),
 FOREIGN KEY(individual_id,evaluation_run_id) REFERENCES runs(individual_id,id),
 FOREIGN KEY(individual_id,result_event_id) REFERENCES events(individual_id,id),
 CHECK((dispatch_mode IS NULL)=(attested_at IS NULL)),
 CHECK((request_digest IS NULL AND context_json IS NULL AND input_bound IS NULL AND output_cap IS NULL)
 OR (request_digest ~ '^[0-9a-f]{64}$' AND context_json IS NOT NULL AND input_bound IS NOT NULL AND output_cap IS NOT NULL))
);
CREATE TABLE development_trial_case_sources (
 individual_id uuid NOT NULL, case_id uuid NOT NULL, event_id uuid NOT NULL,
 PRIMARY KEY(individual_id,case_id,event_id),
 FOREIGN KEY(individual_id,case_id) REFERENCES development_trial_cases(individual_id,id),
 FOREIGN KEY(individual_id,event_id) REFERENCES events(individual_id,id)
);
CREATE INDEX development_trial_pending ON development_trial_cases(individual_id,status,plan_id,ordinal);
