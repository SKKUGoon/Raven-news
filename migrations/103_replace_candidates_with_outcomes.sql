DROP TABLE IF EXISTS warehouse.polymarket_market_candidates;

CREATE TABLE IF NOT EXISTS warehouse.polymarket_market_outcomes (
    market_id TEXT NOT NULL,
    outcome_index INTEGER NOT NULL,
    outcome_label TEXT NOT NULL,
    first_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    PRIMARY KEY (market_id, outcome_index),
    CONSTRAINT fk_polymarket_market_outcomes_market_id
        FOREIGN KEY (market_id)
        REFERENCES warehouse.polymarket_markets(market_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_polymarket_market_outcomes_label
    ON warehouse.polymarket_market_outcomes (outcome_label);
