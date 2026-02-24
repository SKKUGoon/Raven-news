CREATE TABLE IF NOT EXISTS warehouse.polymarket_markets (
    market_id TEXT PRIMARY KEY,
    event_id TEXT,
    event_title TEXT,
    event_description TEXT,
    event_image TEXT,
    question TEXT,
    description TEXT,
    image TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    closed BOOLEAN NOT NULL DEFAULT FALSE,
    maturity_reached BOOLEAN NOT NULL DEFAULT FALSE,
    end_date TIMESTAMP WITH TIME ZONE,
    event_payload TEXT NOT NULL,
    market_payload TEXT NOT NULL,
    first_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_polymarket_markets_event_id
    ON warehouse.polymarket_markets (event_id);

CREATE INDEX IF NOT EXISTS idx_polymarket_markets_last_seen_at
    ON warehouse.polymarket_markets (last_seen_at);

CREATE INDEX IF NOT EXISTS idx_polymarket_markets_maturity_reached
    ON warehouse.polymarket_markets (maturity_reached);
