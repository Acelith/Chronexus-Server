-- S6: revocation, deletion and recovery boundaries.
-- A deleted campaign keeps only its id, the deletion time and its (inactive) membership rows,
-- so former members learn it is gone; every payload, upload, log row, receipt and invitation
-- is removed. A deleted account keeps an anonymous stub: content-free tombstones and change-log
-- rows of the other members' history still reference it.
ALTER TABLE campaigns ADD COLUMN deleted_at INTEGER;
ALTER TABLE accounts ADD COLUMN deleted_at INTEGER;
