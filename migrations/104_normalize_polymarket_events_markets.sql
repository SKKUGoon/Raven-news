CREATE TABLE IF NOT EXISTS warehouse.polymarket_events (
    event_id TEXT PRIMARY KEY,
    event_title TEXT,
    event_description TEXT,
    event_image TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    closed BOOLEAN NOT NULL DEFAULT FALSE,
    first_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

ALTER TABLE warehouse.polymarket_markets
    ADD COLUMN IF NOT EXISTS market_name TEXT,
    ADD COLUMN IF NOT EXISTS group_item_id TEXT;

INSERT INTO warehouse.polymarket_events (
    event_id,
    event_title,
    event_description,
    event_image,
    active,
    closed,
    first_seen_at,
    last_seen_at,
    updated_at
)
SELECT DISTINCT
    m.event_id,
    NULL,
    NULL,
    NULL,
    COALESCE(m.active, TRUE),
    COALESCE(m.closed, FALSE),
    now(),
    now(),
    now()
FROM warehouse.polymarket_markets m
WHERE m.event_id IS NOT NULL
  AND m.event_id <> ''
ON CONFLICT (event_id) DO NOTHING;

UPDATE warehouse.polymarket_markets
SET market_name = COALESCE(market_name, question)
WHERE market_name IS NULL;

ALTER TABLE warehouse.polymarket_markets
    DROP COLUMN IF EXISTS event_title,
    DROP COLUMN IF EXISTS event_description,
    DROP COLUMN IF EXISTS event_image,
    DROP COLUMN IF EXISTS group_item_title,
    DROP COLUMN IF EXISTS odds_value,
    DROP COLUMN IF EXISTS question;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_polymarket_markets_event_id'
    ) THEN
        ALTER TABLE warehouse.polymarket_markets
            ADD CONSTRAINT fk_polymarket_markets_event_id
            FOREIGN KEY (event_id)
            REFERENCES warehouse.polymarket_events(event_id)
            ON DELETE CASCADE;
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS idx_polymarket_events_last_seen_at
    ON warehouse.polymarket_events (last_seen_at);
