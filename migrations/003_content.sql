ALTER TABLE campaigns ADD COLUMN next_seq INTEGER NOT NULL DEFAULT 0;
CREATE TABLE content (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id),
    kind TEXT NOT NULL CHECK(kind IN ('page','session','relationship','party','sheetSection','image')),
    id TEXT NOT NULL,
    author_id TEXT NOT NULL REFERENCES accounts(id),
    revision INTEGER NOT NULL CHECK(revision>0),
    visibility TEXT NOT NULL CHECK(visibility IN ('private','members')),
    payload TEXT,
    deleted INTEGER NOT NULL CHECK(deleted IN (0,1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(campaign_id,kind,id),
    CHECK((deleted=1 AND payload IS NULL) OR (deleted=0 AND payload IS NOT NULL))
) STRICT;
CREATE INDEX content_author ON content(campaign_id,author_id,visibility);
CREATE TABLE published_items (
    campaign_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    deleted INTEGER NOT NULL CHECK(deleted IN (0,1)),
    seq INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(campaign_id,kind,id),
    FOREIGN KEY(campaign_id,kind,id) REFERENCES content(campaign_id,kind,id)
) STRICT;
CREATE INDEX published_feed ON published_items(campaign_id,seq);
CREATE TABLE content_dependencies (
    campaign_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    item_id TEXT NOT NULL,
    page_id TEXT NOT NULL,
    PRIMARY KEY(campaign_id,kind,item_id,page_id),
    FOREIGN KEY(campaign_id,kind,item_id) REFERENCES content(campaign_id,kind,id)
) STRICT;
CREATE INDEX dependency_target ON content_dependencies(campaign_id,page_id);
CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id),
    author_id TEXT NOT NULL REFERENCES accounts(id),
    sha256 TEXT NOT NULL,
    mime TEXT NOT NULL CHECK(mime IN ('image/png','image/jpeg','image/webp')),
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    byte_length INTEGER NOT NULL,
    original BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    CHECK(length(original)=byte_length)
) STRICT;
CREATE INDEX attachments_owner ON attachments(campaign_id,author_id);
CREATE TABLE content_attachments (
    campaign_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    item_id TEXT NOT NULL,
    attachment_id TEXT NOT NULL REFERENCES attachments(id),
    PRIMARY KEY(campaign_id,kind,item_id,attachment_id),
    FOREIGN KEY(campaign_id,kind,item_id) REFERENCES content(campaign_id,kind,id)
) STRICT;
