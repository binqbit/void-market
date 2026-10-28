CREATE TABLE node_identity (
    node_id UUID PRIMARY KEY,
    instance_id UUID NOT NULL,
    public_key TEXT NOT NULL,
    heartbeat_sequence BIGINT NOT NULL DEFAULT 0 CHECK (heartbeat_sequence >= 0)
);
CREATE TABLE node_attempts (
    node_id UUID NOT NULL REFERENCES node_identity(node_id),
    job_id UUID NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    status TEXT NOT NULL CHECK (status IN ('running','waiting','result_pending','completed','interrupted','failed','unavailable')),
    pending_result JSONB,
    CHECK ((status = 'result_pending') = (pending_result IS NOT NULL)),
    PRIMARY KEY (node_id, job_id)
);
