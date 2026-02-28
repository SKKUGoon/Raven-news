ALTER TABLE warehouse.polymarket_events
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMP WITH TIME ZONE;

UPDATE warehouse.polymarket_events
SET created_at = COALESCE(created_at, updated_at, now())
WHERE created_at IS NULL;

ALTER TABLE warehouse.polymarket_events
    ALTER COLUMN created_at SET NOT NULL,
    DROP COLUMN IF EXISTS event_description,
    DROP COLUMN IF EXISTS event_image,
    DROP COLUMN IF EXISTS first_seen_at,
    DROP COLUMN IF EXISTS last_seen_at;
