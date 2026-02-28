ALTER TABLE warehouse.polymarket_events
    ADD COLUMN IF NOT EXISTS end_date TIMESTAMP WITH TIME ZONE;
