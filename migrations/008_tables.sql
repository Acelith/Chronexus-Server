CREATE TABLE game_tables (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    gm_id TEXT NOT NULL REFERENCES accounts(id),
    created_at INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX game_tables_gm ON game_tables(gm_id);
CREATE TABLE table_seats (
    table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    role TEXT NOT NULL CHECK (role IN ('gm','player','spectator')),
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (table_id, account_id)
) STRICT;
CREATE INDEX table_seats_account ON table_seats(account_id);
CREATE TABLE table_invites (
    code_hash TEXT PRIMARY KEY,
    table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('player','spectator')),
    created_by TEXT NOT NULL REFERENCES accounts(id),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at INTEGER,
    used_by TEXT REFERENCES accounts(id)
) STRICT;
