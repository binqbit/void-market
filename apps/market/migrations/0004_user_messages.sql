CREATE SEQUENCE message_revision;

CREATE TABLE custody_keys (
    account_id UUID PRIMARY KEY REFERENCES accounts(id),
    signing_key BYTEA NOT NULL,
    signing_public_key TEXT NOT NULL,
    he_status TEXT NOT NULL DEFAULT 'queued' CHECK (he_status IN ('queued','building','ready','failed')),
    he_material JSONB,
    he_secret BYTEA,
    lease_until TIMESTAMPTZ,
    lease_id UUID,
    CHECK ((he_status = 'ready') = (he_material IS NOT NULL AND he_secret IS NOT NULL))
);

CREATE TABLE approval_runtime (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    instance_id UUID NOT NULL,
    last_tick_ms BIGINT NOT NULL,
    healthy BOOLEAN NOT NULL
);

CREATE TABLE approval_sessions (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 128),
    node_id UUID NOT NULL REFERENCES arbiter_nodes(id),
    job_id UUID UNIQUE REFERENCES arbiter_jobs(id),
    status TEXT NOT NULL DEFAULT 'initializing' CHECK (status IN ('initializing','active','completed','interrupted')),
    comparison TEXT NOT NULL,
    width SMALLINT NOT NULL CHECK (width IN (8,16,32,64)),
    current_request JSONB,
    check_context JSONB,
    response_ref JSONB,
    generation BIGINT NOT NULL DEFAULT 0,
    clock_epoch UUID,
    last_tick_ms BIGINT,
    revision BIGINT NOT NULL DEFAULT nextval('message_revision'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE approval_members (
    session_id UUID NOT NULL REFERENCES approval_sessions(id),
    account_id UUID NOT NULL REFERENCES accounts(id),
    position SMALLINT NOT NULL CHECK (position IN (0,1)),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','withdrawn','disqualified')),
    input_sealed BYTEA NOT NULL,
    remaining_ms BIGINT NOT NULL DEFAULT 600000 CHECK (remaining_ms BETWEEN 0 AND 600000),
    chargeable BOOLEAN NOT NULL DEFAULT FALSE,
    exit_receipt JSONB,
    PRIMARY KEY (session_id,account_id),
    UNIQUE (session_id,position)
);

CREATE TABLE approval_messages (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES approval_sessions(id),
    account_id UUID NOT NULL REFERENCES accounts(id),
    request_id UUID NOT NULL,
    request_ref JSONB NOT NULL,
    generation BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('number_transfer','mask_contribution','answer_commitment','answer_reveal','mask_opening')),
    status TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('ready','approved','queued','processing','response_pending','responded','completed','cancelled','blocked')),
    response_ref JSONB,
    private_state BYTEA,
    approval JSONB,
    lease_until TIMESTAMPTZ,
    lease_id UUID,
    revision BIGINT NOT NULL DEFAULT nextval('message_revision'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (session_id,request_id,account_id,kind)
);
CREATE INDEX approval_messages_account ON approval_messages(account_id,created_at,id);
ALTER TABLE arbiter_jobs ADD COLUMN cancel_requested BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE arbiter_jobs ADD COLUMN interactive BOOLEAN NOT NULL DEFAULT FALSE;
