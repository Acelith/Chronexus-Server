CREATE TABLE table_live_tickets (
    ticket_hash TEXT PRIMARY KEY,
    table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    session_id TEXT NOT NULL REFERENCES sessions(id),
    expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX table_live_tickets_expiry ON table_live_tickets(expires_at);
CREATE TABLE table_live_events (
    table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL,
    operation_id TEXT NOT NULL,
    base_revision INTEGER NOT NULL,
    author_id TEXT NOT NULL REFERENCES accounts(id),
    audience TEXT NOT NULL CHECK(audience IN ('public','gm')),
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(table_id,revision),
    UNIQUE(table_id,operation_id)
) STRICT;
CREATE INDEX table_live_events_recent ON table_live_events(table_id,revision DESC);
