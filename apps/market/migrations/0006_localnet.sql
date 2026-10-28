ALTER TABLE arbiter_nodes ADD COLUMN ethereum_address TEXT;
ALTER TABLE accounts ADD COLUMN is_demo BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE accounts DROP CONSTRAINT accounts_identity;
ALTER TABLE accounts ADD CONSTRAINT accounts_identity CHECK (passkey IS NOT NULL OR google_subject IS NOT NULL OR is_demo);
CREATE UNIQUE INDEX arbiter_nodes_ethereum_address ON arbiter_nodes(ethereum_address)
    WHERE ethereum_address IS NOT NULL;

ALTER TABLE assets ADD COLUMN nft JSONB;
ALTER TABLE auctions ADD COLUMN execution TEXT NOT NULL DEFAULT 'internal'
    CHECK (execution IN ('internal','localnet'));
ALTER TABLE auctions ADD COLUMN custody_status TEXT NOT NULL DEFAULT 'none'
    CHECK (custody_status IN ('none','deposit_pending','held','return_pending','returned','delivery_pending','delivered','review'));
ALTER TABLE auctions ADD COLUMN payment_status TEXT NOT NULL DEFAULT 'unverified'
    CHECK (payment_status IN ('unverified','pending','confirmed','review'));
ALTER TABLE auctions ADD COLUMN settlement_note TEXT;
ALTER TABLE auctions ADD COLUMN payment_invoice BYTEA;
ALTER TABLE auctions DROP CONSTRAINT auctions_status_check;
ALTER TABLE auctions ADD CONSTRAINT auctions_status_check CHECK (status IN
    ('draft','deposit_pending','open','matching','awaiting_completion','completed','no_sale','interrupted','cancelled'));
DROP INDEX auctions_active_asset;
CREATE UNIQUE INDEX auctions_active_asset ON auctions(asset_id) WHERE
    status IN ('deposit_pending','open','matching','awaiting_completion') OR
    custody_status IN ('deposit_pending','held','return_pending','delivery_pending','review');

CREATE TABLE localnet_config (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    ethereum_genesis TEXT NOT NULL,
    collection_address TEXT,
    collection_code_hash TEXT NOT NULL,
    deployment_transaction TEXT,
    signed_deployment TEXT,
    zcash_genesis TEXT NOT NULL,
    minter_address TEXT NOT NULL,
    faucet_account UUID NOT NULL,
    faucet_address TEXT NOT NULL
);

CREATE TABLE demo_accounts (
    account_id UUID PRIMARY KEY REFERENCES accounts(id),
    alias TEXT NOT NULL UNIQUE CHECK (alias IN ('alice','bob','daniel')),
    ethereum_address TEXT NOT NULL UNIQUE,
    zcash_account UUID UNIQUE,
    zcash_address TEXT UNIQUE
);

CREATE TABLE chain_operations (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id),
    recipient_id UUID REFERENCES accounts(id),
    auction_id UUID REFERENCES auctions(id),
    asset_id UUID REFERENCES assets(id),
    kind TEXT NOT NULL CHECK (kind IN ('mint','airdrop','deposit','return','delivery','transfer','payment')),
    status TEXT NOT NULL DEFAULT 'preparing' CHECK (status IN
        ('preparing','ready','approved','building','signed','submitted','confirmed','failed','reconcile','cancelled')),
    payload_sealed BYTEA NOT NULL,
    pczt_sealed BYTEA,
    signed_transaction TEXT,
    transaction_id TEXT,
    receipt_sealed BYTEA,
    approval JSONB,
    error TEXT,
    revision BIGINT NOT NULL DEFAULT nextval('message_revision'),
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);
CREATE UNIQUE INDEX chain_operations_auction_kind ON chain_operations(auction_id,kind)
    WHERE auction_id IS NOT NULL AND kind<>'payment' AND status NOT IN ('failed','cancelled');
CREATE UNIQUE INDEX chain_operations_active_payment ON chain_operations(auction_id)
    WHERE kind='payment' AND status IN ('preparing','ready','approved','building','signed','submitted','reconcile');
CREATE INDEX chain_operations_account ON chain_operations(account_id,created_at_ms DESC);
CREATE INDEX chain_operations_pending ON chain_operations(status,created_at_ms)
    WHERE status NOT IN ('confirmed','failed','cancelled');
ALTER TABLE assets ADD COLUMN pending_transaction_id UUID REFERENCES chain_operations(id);

CREATE TABLE localnet_runtime (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    instance_id UUID NOT NULL,
    last_tick_ms BIGINT NOT NULL,
    healthy BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE payment_credits (
    auction_id UUID NOT NULL REFERENCES auctions(id),
    operation_id UUID NOT NULL UNIQUE REFERENCES chain_operations(id),
    transaction_id TEXT NOT NULL,
    output_index INTEGER NOT NULL CHECK (output_index>=0),
    value_sealed BYTEA NOT NULL,
    block_hash TEXT NOT NULL,
    block_height BIGINT NOT NULL,
    PRIMARY KEY(transaction_id,output_index)
);
