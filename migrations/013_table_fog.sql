-- Table visibility is independent of journal content and campaign backups.
CREATE TABLE table_fog (
 scene_id TEXT PRIMARY KEY REFERENCES table_scenes(id) ON DELETE CASCADE,
 table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 data TEXT NOT NULL
) STRICT;
CREATE INDEX table_fog_table ON table_fog(table_id);
