-- S5: per-campaign change log for resumable private/shared incremental reads.
-- Rows carry no payload. The feed always reads the current authorized state, so a
-- log row can only signal that an item should be re-examined by a given account.
-- state: private | members | deleted-private | deleted | withdrawn. The last three
-- and `members` are the only states visible to non-authors; they never name private edits.
CREATE TABLE content_log (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id),
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    author_id TEXT NOT NULL REFERENCES accounts(id),
    state TEXT NOT NULL CHECK(state IN ('private','members','deleted-private','deleted','withdrawn')),
    at INTEGER NOT NULL,
    PRIMARY KEY(campaign_id,seq)
) STRICT;
-- Cursors below log_floor have lost rows and must resynchronize from a full listing.
ALTER TABLE campaigns ADD COLUMN log_floor INTEGER NOT NULL DEFAULT 0;
INSERT INTO content_log(campaign_id,seq,kind,id,author_id,state,at)
SELECT c.campaign_id,
       k.next_seq + row_number() OVER (PARTITION BY c.campaign_id ORDER BY c.updated_at,c.kind,c.id),
       c.kind,c.id,c.author_id,
       CASE
         WHEN c.deleted=1 AND EXISTS(SELECT 1 FROM published_items p WHERE p.campaign_id=c.campaign_id AND p.kind=c.kind AND p.id=c.id) THEN 'deleted'
         WHEN c.deleted=1 THEN 'deleted-private'
         WHEN c.visibility='members' THEN 'members'
         WHEN EXISTS(SELECT 1 FROM published_items p WHERE p.campaign_id=c.campaign_id AND p.kind=c.kind AND p.id=c.id) THEN 'withdrawn'
         ELSE 'private'
       END,
       c.updated_at
FROM content c JOIN campaigns k ON k.id=c.campaign_id;
UPDATE campaigns SET next_seq=next_seq+(SELECT count(*) FROM content_log l WHERE l.campaign_id=campaigns.id);
-- Receipts now cover every keyed mutation and expire with the request-ID age window.
ALTER TABLE save_receipts ADD COLUMN op TEXT NOT NULL DEFAULT 'save' CHECK(op IN ('save','delete','unshare'));
ALTER TABLE save_receipts ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
UPDATE save_receipts SET created_at=CAST(strftime('%s','now') AS INTEGER);
CREATE INDEX save_receipts_age ON save_receipts(campaign_id,account_id,created_at);
