ALTER TABLE warehouse.polymarket_markets
    DROP COLUMN IF EXISTS maturity_reached,
    DROP COLUMN IF EXISTS event_payload,
    DROP COLUMN IF EXISTS market_payload,
    ADD COLUMN IF NOT EXISTS group_item_title TEXT,
    ADD COLUMN IF NOT EXISTS odds_value DOUBLE PRECISION;

DROP INDEX IF EXISTS idx_polymarket_markets_maturity_reached;

CREATE TABLE IF NOT EXISTS warehouse.polymarket_market_candidates (
    market_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    candidate_title TEXT NOT NULL,
    first_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    PRIMARY KEY (market_id, candidate_id),
    CONSTRAINT fk_polymarket_market_candidates_market_id
        FOREIGN KEY (market_id)
        REFERENCES warehouse.polymarket_markets(market_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_polymarket_market_candidates_title
    ON warehouse.polymarket_market_candidates (candidate_title);
