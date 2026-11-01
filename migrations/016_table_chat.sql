-- Server-rolled chat dice. NULL for ordinary messages; existing events are unchanged.
ALTER TABLE table_live_events ADD COLUMN roll TEXT CHECK(roll IS NULL OR json_valid(roll));
