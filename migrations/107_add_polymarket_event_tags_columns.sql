ALTER TABLE warehouse.polymarket_events
    ADD COLUMN IF NOT EXISTS event_tag_slugs TEXT,
    ADD COLUMN IF NOT EXISTS event_tags TEXT,
    ADD COLUMN IF NOT EXISTS event_tag_labels TEXT;
