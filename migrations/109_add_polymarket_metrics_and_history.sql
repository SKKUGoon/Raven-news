ALTER TABLE warehouse.polymarket_events
    ADD COLUMN IF NOT EXISTS total_volume DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS volume_24h DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS open_interest DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS liquidity DOUBLE PRECISION;

CREATE TABLE IF NOT EXISTS warehouse.polymarket_event_metrics_history (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    captured_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    total_volume DOUBLE PRECISION,
    volume_24h DOUBLE PRECISION,
    open_interest DOUBLE PRECISION,
    liquidity DOUBLE PRECISION,
    CONSTRAINT fk_polymarket_event_metrics_history_event_id
        FOREIGN KEY (event_id)
        REFERENCES warehouse.polymarket_events(event_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_polymarket_event_metrics_history_event_time
    ON warehouse.polymarket_event_metrics_history (event_id, captured_at DESC);

CREATE INDEX IF NOT EXISTS idx_polymarket_event_metrics_history_captured_at
    ON warehouse.polymarket_event_metrics_history (captured_at DESC);
