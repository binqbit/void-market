CREATE TABLE auctions (
    id UUID PRIMARY KEY,
    asset_id UUID NOT NULL REFERENCES assets(id),
    seller_id UUID NOT NULL REFERENCES accounts(id),
    node_id UUID REFERENCES arbiter_nodes(id),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN
        ('draft','open','matching','awaiting_completion','completed','no_sale','interrupted','cancelled')),
    closes_at_ms BIGINT NOT NULL,
    width SMALLINT NOT NULL DEFAULT 64 CHECK (width IN (8,16,32,64)),
    buyer_limit SMALLINT NOT NULL DEFAULT 3 CHECK (buyer_limit BETWEEN 1 AND 32),
    asset_snapshot JSONB NOT NULL,
    creation_hash BYTEA NOT NULL,
    policy_hash BYTEA,
    publish_receipt JSONB,
    input_root BYTEA,
    round_root BYTEA,
    generation BIGINT NOT NULL DEFAULT 0,
    winner_id UUID REFERENCES accounts(id),
    completion_request_id UUID,
    completion_receipt JSONB,
    award_hash BYTEA,
    reason TEXT,
    clock_epoch UUID,
    last_tick_ms BIGINT,
    revision BIGINT NOT NULL DEFAULT nextval('message_revision'),
    created_at_ms BIGINT NOT NULL,
    published_at_ms BIGINT,
    finished_at_ms BIGINT
);
CREATE UNIQUE INDEX auctions_active_asset ON auctions(asset_id)
    WHERE status IN ('open','matching','awaiting_completion');
CREATE INDEX auctions_market ON auctions(status, closes_at_ms, id);

CREATE TABLE auction_participants (
    auction_id UUID NOT NULL REFERENCES auctions(id),
    account_id UUID NOT NULL REFERENCES accounts(id),
    id UUID NOT NULL UNIQUE,
    admission_seq BIGSERIAL UNIQUE,
    input_sealed BYTEA NOT NULL,
    frozen_input JSONB,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','withdrawn','disqualified')),
    remaining_ms BIGINT NOT NULL DEFAULT 600000 CHECK (remaining_ms BETWEEN 0 AND 600000),
    chargeable BOOLEAN NOT NULL DEFAULT FALSE,
    exit_receipt JSONB,
    admission_receipt JSONB,
    created_at_ms BIGINT NOT NULL,
    PRIMARY KEY (auction_id, account_id)
);
CREATE INDEX auction_participants_account ON auction_participants(account_id, auction_id);

ALTER TABLE approval_sessions ADD COLUMN auction_id UUID REFERENCES auctions(id);
ALTER TABLE approval_sessions ADD COLUMN round_generation BIGINT NOT NULL DEFAULT 0;
ALTER TABLE approval_sessions ADD COLUMN scope_hash BYTEA;
ALTER TABLE approval_members ALTER COLUMN input_sealed DROP NOT NULL;
ALTER TABLE approval_members ADD COLUMN input_ref JSONB;
ALTER TABLE approval_members ADD CONSTRAINT approval_member_input CHECK
    ((input_sealed IS NULL) <> (input_ref IS NULL));

CREATE TABLE auction_checks (
    session_id UUID PRIMARY KEY REFERENCES approval_sessions(id),
    auction_id UUID NOT NULL REFERENCES auctions(id),
    generation BIGINT NOT NULL,
    slot INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('pair','reserve')),
    left_account UUID NOT NULL,
    right_account UUID NOT NULL,
    round_root BYTEA NOT NULL,
    result BOOLEAN,
    result_ref JSONB,
    UNIQUE (auction_id, generation, slot),
    FOREIGN KEY (auction_id, left_account) REFERENCES auction_participants(auction_id, account_id),
    FOREIGN KEY (auction_id, right_account) REFERENCES auction_participants(auction_id, account_id),
    CHECK (left_account <> right_account),
    CHECK ((result IS NULL) = (result_ref IS NULL))
);
ALTER TABLE transfers ADD COLUMN auction_id UUID UNIQUE REFERENCES auctions(id);
