CREATE TABLE table_portrait_owners (
 asset_id TEXT NOT NULL REFERENCES table_map_assets(id) ON DELETE CASCADE,
 account_id TEXT NOT NULL REFERENCES accounts(id),
 PRIMARY KEY(asset_id,account_id)
) STRICT;
