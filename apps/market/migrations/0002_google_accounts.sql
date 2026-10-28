-- Retain existing passkey identities and asset ownership during the transition.
ALTER TABLE accounts ALTER COLUMN credential_id DROP NOT NULL;
ALTER TABLE accounts ALTER COLUMN passkey DROP NOT NULL;
ALTER TABLE accounts ADD COLUMN google_subject TEXT UNIQUE;
ALTER TABLE accounts ADD COLUMN email TEXT;
ALTER TABLE accounts ADD COLUMN username TEXT UNIQUE
    CHECK (username ~ '^[a-z][a-z0-9_]{2,23}$');

CREATE UNIQUE INDEX accounts_email ON accounts (lower(email));
ALTER TABLE accounts ADD CONSTRAINT accounts_identity
    CHECK (passkey IS NOT NULL OR google_subject IS NOT NULL);
