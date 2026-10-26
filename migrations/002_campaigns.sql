CREATE TABLE campaigns (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id TEXT NOT NULL REFERENCES accounts(id),
    created_at INTEGER NOT NULL
) STRICT;
CREATE TABLE members (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    role TEXT NOT NULL CHECK(role IN ('owner','player','reader')),
    active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    joined_at INTEGER NOT NULL,
    PRIMARY KEY(campaign_id,account_id)
) STRICT;
CREATE UNIQUE INDEX one_campaign_owner ON members(campaign_id) WHERE role='owner' AND active=1;
CREATE INDEX memberships_account ON members(account_id,campaign_id);
CREATE TABLE campaign_invites (
    id TEXT PRIMARY KEY,
    code_hash TEXT NOT NULL UNIQUE,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id),
    role TEXT NOT NULL CHECK(role IN ('player','reader')),
    created_by TEXT NOT NULL REFERENCES accounts(id),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_by TEXT REFERENCES accounts(id),
    used_at INTEGER,
    revoked_at INTEGER
) STRICT;
CREATE INDEX campaign_invites_campaign ON campaign_invites(campaign_id,expires_at);
