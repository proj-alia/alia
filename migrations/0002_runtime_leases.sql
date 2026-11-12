ALTER TABLE individuals ADD COLUMN lease_generation bigint NOT NULL DEFAULT 0 CHECK (lease_generation>=0);
ALTER TABLE individuals ADD COLUMN autonomy_paused boolean NOT NULL DEFAULT false;
ALTER TABLE individuals ADD COLUMN budget_frozen boolean NOT NULL DEFAULT false;
ALTER TABLE runs ADD COLUMN lease_owner uuid;
ALTER TABLE runs ADD COLUMN lease_generation bigint NOT NULL DEFAULT 0 CHECK (lease_generation>=0);
ALTER TABLE runs ADD COLUMN lease_expires_at timestamptz;
ALTER TABLE runs ADD COLUMN deadline timestamptz;
ALTER TABLE runs ADD COLUMN model_calls integer NOT NULL DEFAULT 0 CHECK (model_calls>=0);
ALTER TABLE runs ADD COLUMN tool_calls integer NOT NULL DEFAULT 0 CHECK (tool_calls>=0);
ALTER TABLE runs ADD COLUMN error_code text;
ALTER TABLE runs ADD COLUMN finished_at timestamptz;
CREATE UNIQUE INDEX one_running_individual ON runs(individual_id) WHERE status='running';
CREATE INDEX run_queue ON runs(status,mode,created_at);

CREATE TABLE budget_windows (
    id uuid PRIMARY KEY,
    individual_id uuid NOT NULL REFERENCES individuals(id),
    timezone text NOT NULL,
    starts_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL CHECK (ends_at>starts_at),
    charged_total bigint NOT NULL DEFAULT 0 CHECK (charged_total>=0),
    charged_background bigint NOT NULL DEFAULT 0 CHECK (charged_background>=0 AND charged_background<=charged_total),
    UNIQUE(individual_id,starts_at)
);
CREATE TABLE usage_reservations (
    id uuid PRIMARY KEY,
    individual_id uuid NOT NULL,
    run_id uuid NOT NULL,
    lease_generation bigint NOT NULL,
    request_key uuid NOT NULL,
    request_digest text NOT NULL,
    window_id uuid NOT NULL REFERENCES budget_windows(id),
    mode text NOT NULL CHECK (mode IN ('foreground','background')),
    input_bound bigint NOT NULL CHECK(input_bound>0),
    output_cap bigint NOT NULL CHECK(output_cap BETWEEN 1 AND 8192),
    reserved_tokens bigint NOT NULL CHECK(reserved_tokens=input_bound+output_cap),
    status text NOT NULL CHECK(status IN ('reserved','dispatched','settled')),
    charged_tokens bigint NOT NULL DEFAULT 0 CHECK(charged_tokens>=0),
    usage_unknown boolean NOT NULL DEFAULT false,
    actual_input bigint,
    actual_output bigint,
    actual_total bigint,
    created_at timestamptz NOT NULL DEFAULT now(),
    dispatched_at timestamptz,
    settled_at timestamptz,
    FOREIGN KEY(individual_id,run_id) REFERENCES runs(individual_id,id),
    UNIQUE(run_id,request_key)
);
CREATE UNIQUE INDEX one_outstanding_model_call ON usage_reservations(run_id) WHERE status IN ('reserved','dispatched');
CREATE INDEX unsettled_window_usage ON usage_reservations(window_id,status);
