-- Content-free receipts commit atomically with accepted saves. Never evict silently:
-- a retried operation must not resurrect content after a withdrawal or deletion.
CREATE TABLE save_receipts (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    request_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    item_id TEXT NOT NULL,
    request_hash TEXT NOT NULL CHECK(length(request_hash)=64),
    revision INTEGER NOT NULL CHECK(revision>0),
    PRIMARY KEY(campaign_id,account_id,request_id)
) STRICT;
