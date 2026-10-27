-- Rebuild only the content CHECK; preserve every existing row and dependent key.
CREATE TABLE content_v7 (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id),
    kind TEXT NOT NULL CHECK(kind IN ('page','session','relationship','party','sheetSection','image','clue')),
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
INSERT INTO content_v7 SELECT * FROM content;
DROP TABLE content;
ALTER TABLE content_v7 RENAME TO content;
CREATE INDEX content_author ON content(campaign_id,author_id,visibility);
CREATE TABLE clue_session_dependencies (
    campaign_id TEXT NOT NULL,
    clue_kind TEXT NOT NULL DEFAULT 'clue' CHECK(clue_kind='clue'),
    clue_id TEXT NOT NULL,
    session_kind TEXT NOT NULL DEFAULT 'session' CHECK(session_kind='session'),
    session_id TEXT NOT NULL,
    PRIMARY KEY(campaign_id,clue_id,session_id),
    FOREIGN KEY(campaign_id,clue_kind,clue_id) REFERENCES content(campaign_id,kind,id),
    FOREIGN KEY(campaign_id,session_kind,session_id) REFERENCES content(campaign_id,kind,id)
) STRICT;
CREATE INDEX clue_dependency_session ON clue_session_dependencies(campaign_id,session_id);
