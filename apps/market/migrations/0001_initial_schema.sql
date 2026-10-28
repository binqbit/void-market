CREATE TABLE accounts (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 64),
    credential_id BYTEA NOT NULL UNIQUE,
    passkey JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    token_hash BYTEA PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_expiration ON sessions(expires_at);

CREATE TABLE assets (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 128),
    description TEXT NOT NULL CHECK (char_length(description) <= 2048),
    image TEXT,
    owner_id UUID NOT NULL REFERENCES accounts(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX assets_owner ON assets(owner_id, created_at, id);

CREATE TABLE transfers (
    id UUID PRIMARY KEY,
    asset_id UUID NOT NULL REFERENCES assets(id),
    sender_id UUID NOT NULL REFERENCES accounts(id),
    recipient_id UUID NOT NULL REFERENCES accounts(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (sender_id <> recipient_id)
);
CREATE INDEX transfers_sender ON transfers(sender_id, created_at, id);
CREATE INDEX transfers_recipient ON transfers(recipient_id, created_at, id);

CREATE TABLE outbox (
    id UUID PRIMARY KEY,
    kind TEXT NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ
);
CREATE INDEX outbox_pending ON outbox(created_at, id) WHERE published_at IS NULL;
