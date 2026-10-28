ALTER TABLE assets ALTER COLUMN owner_id DROP NOT NULL;
ALTER TABLE assets ADD CONSTRAINT assets_custody_owner
    CHECK (owner_id IS NOT NULL OR nft IS NOT NULL);

CREATE UNIQUE INDEX assets_nft_identity ON assets (
    (nft->>'genesis'), lower(nft->>'collection'), (nft->>'token_id')
) WHERE nft IS NOT NULL;

ALTER TABLE chain_operations DROP CONSTRAINT chain_operations_kind_check;
ALTER TABLE chain_operations ADD CONSTRAINT chain_operations_kind_check
    CHECK (kind IN ('mint','airdrop','deposit','return','delivery','transfer','payment','withdrawal','nft_deposit'));

CREATE UNIQUE INDEX chain_operations_nft_deposit ON chain_operations(transaction_id,asset_id)
    WHERE kind='nft_deposit';
