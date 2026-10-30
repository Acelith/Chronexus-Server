-- Table-only assets and state. Nothing references or mutates journal content.
CREATE TABLE table_map_assets (
 id TEXT PRIMARY KEY, table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 sha256 TEXT NOT NULL, mime TEXT NOT NULL, width INTEGER NOT NULL, height INTEGER NOT NULL,
 original BLOB NOT NULL, UNIQUE(table_id,sha256)
) STRICT;
CREATE TABLE table_scenes (
 id TEXT PRIMARY KEY, table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 data TEXT NOT NULL, geometry TEXT NOT NULL DEFAULT '{}'
) STRICT;
CREATE INDEX table_scenes_table ON table_scenes(table_id);
CREATE TABLE table_tokens (
 id TEXT PRIMARY KEY, table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 scene_id TEXT NOT NULL REFERENCES table_scenes(id) ON DELETE CASCADE, data TEXT NOT NULL
) STRICT;
CREATE INDEX table_tokens_scene ON table_tokens(table_id,scene_id);
CREATE TABLE table_boards (
 table_id TEXT PRIMARY KEY REFERENCES game_tables(id) ON DELETE CASCADE,
 active_scene TEXT REFERENCES table_scenes(id) ON DELETE SET NULL
) STRICT;
CREATE TABLE table_operation_receipts (
 table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 operation_id TEXT NOT NULL, account_id TEXT NOT NULL REFERENCES accounts(id),
 payload TEXT NOT NULL, revision INTEGER NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(table_id,operation_id)
) STRICT;
CREATE INDEX table_operation_rate ON table_operation_receipts(table_id,account_id,created_at);
CREATE TABLE table_templates (
 id TEXT PRIMARY KEY, table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 scene_id TEXT NOT NULL REFERENCES table_scenes(id) ON DELETE CASCADE,
 owner_id TEXT NOT NULL REFERENCES accounts(id), data TEXT NOT NULL
) STRICT;
CREATE INDEX table_templates_scene ON table_templates(table_id,scene_id);
