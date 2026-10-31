CREATE TABLE table_library (
 id TEXT PRIMARY KEY,
 table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('monster','encounter','asset')),
 data TEXT NOT NULL
) STRICT;
CREATE INDEX table_library_table_kind ON table_library(table_id,kind);
