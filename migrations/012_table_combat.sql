-- The combat state never writes character sheets or journal items.
CREATE TABLE table_combats (
 table_id TEXT PRIMARY KEY REFERENCES game_tables(id) ON DELETE CASCADE, data TEXT NOT NULL
) STRICT;
CREATE TABLE table_token_vitals (
 token_id TEXT PRIMARY KEY REFERENCES table_tokens(id) ON DELETE CASCADE,
 table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 hp INTEGER NOT NULL, max_hp INTEGER NOT NULL, ac INTEGER,
 show_hp INTEGER NOT NULL CHECK(show_hp IN (0,1)), gm_note TEXT, status TEXT
) STRICT;
CREATE TABLE table_combat_log (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 token_id TEXT, scene_id TEXT, audience TEXT NOT NULL CHECK(audience IN ('public','gm','owner')),
 owner_id TEXT REFERENCES accounts(id), data TEXT NOT NULL
) STRICT;
CREATE INDEX table_combat_log_recent ON table_combat_log(table_id,id);
