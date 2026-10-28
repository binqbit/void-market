CREATE TABLE arbiter_nodes (
    id UUID PRIMARY KEY,
    deployment_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 64),
    public_key TEXT NOT NULL,
    token_hash BYTEA NOT NULL CHECK (octet_length(token_hash) = 32),
    instance_id UUID,
    heartbeat_sequence BIGINT NOT NULL DEFAULT 0,
    capacity SMALLINT NOT NULL DEFAULT 1 CHECK (capacity BETWEEN 1 AND 4),
    heartbeat_until TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE arbiter_jobs (
    id UUID PRIMARY KEY,
    node_id UUID NOT NULL REFERENCES arbiter_nodes(id),
    input JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','queued','running','waiting','result_pending','completed','interrupted','failed','unavailable')),
    generation BIGINT NOT NULL DEFAULT 0,
    instance_id UUID,
    lease_until TIMESTAMPTZ,
    response JSONB,
    pending_response JSONB,
    output JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX arbiter_jobs_poll ON arbiter_jobs(node_id, created_at, id);

CREATE TABLE arbiter_requests (
    node_id UUID NOT NULL REFERENCES arbiter_nodes(id),
    request_id UUID NOT NULL,
    digest BYTEA NOT NULL,
    response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (node_id, request_id)
);

CREATE TABLE arbiter_results (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES arbiter_jobs(id),
    payload JSONB NOT NULL,
    applied BOOLEAN NOT NULL DEFAULT FALSE
);

ALTER TABLE outbox ADD COLUMN stream TEXT NOT NULL DEFAULT 'assets' CHECK (stream IN ('assets','nodes'));
ALTER TABLE outbox ADD COLUMN claim_id UUID;
ALTER TABLE outbox ADD COLUMN claim_until TIMESTAMPTZ;
CREATE TABLE node_event_inbox (id UUID PRIMARY KEY, digest BYTEA NOT NULL, received_at TIMESTAMPTZ NOT NULL DEFAULT now());
