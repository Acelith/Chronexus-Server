ALTER TABLE accounts ADD COLUMN gm_enabled INTEGER NOT NULL DEFAULT 0 CHECK (gm_enabled IN (0,1));
ALTER TABLE invites ADD COLUMN grant_gm INTEGER NOT NULL DEFAULT 0 CHECK (grant_gm IN (0,1));
UPDATE accounts SET gm_enabled=1 WHERE id IN (SELECT gm_id FROM game_tables);
CREATE TABLE gm_item_grants (
    table_id TEXT NOT NULL REFERENCES game_tables(id) ON DELETE CASCADE,
    campaign_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    item_id TEXT NOT NULL,
    author_id TEXT NOT NULL REFERENCES accounts(id),
    granted_at INTEGER NOT NULL,
    PRIMARY KEY(table_id,campaign_id,kind,item_id),
    FOREIGN KEY(campaign_id,kind,item_id) REFERENCES content(campaign_id,kind,id)
) STRICT;
CREATE INDEX gm_item_grants_campaign ON gm_item_grants(campaign_id,kind,item_id);
CREATE TRIGGER gm_grants_revoke_on_content_withdrawal
AFTER UPDATE OF visibility,deleted ON content
WHEN NEW.visibility!='members' OR NEW.deleted=1
BEGIN
    DELETE FROM gm_item_grants WHERE campaign_id=NEW.campaign_id AND kind=NEW.kind AND item_id=NEW.id;
END;
