ALTER TABLE auction_checks ALTER COLUMN round_root DROP NOT NULL;
ALTER TABLE auction_checks ADD COLUMN precomputed BOOLEAN NOT NULL DEFAULT FALSE;

CREATE UNIQUE INDEX auction_check_pair
    ON auction_checks(auction_id, generation, kind, left_account, right_account);

ALTER TABLE arbiter_nodes ADD COLUMN session_capacity SMALLINT NOT NULL DEFAULT 4
    CHECK (session_capacity BETWEEN capacity AND 64);
