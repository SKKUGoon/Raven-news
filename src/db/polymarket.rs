use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub struct PolymarketMarketRow {
    pub market_id: String,
    pub event_id: Option<String>,
    pub event_title: Option<String>,
    pub event_description: Option<String>,
    pub event_image: Option<String>,
    pub question: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub active: bool,
    pub closed: bool,
    pub maturity_reached: bool,
    pub end_date: Option<DateTime<Utc>>,
}

pub async fn upsert_polymarket_market(
    pool: &PgPool,
    row: &PolymarketMarketRow,
    event_payload: &str,
    market_payload: &str,
    seen_at: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO warehouse.polymarket_markets (
            market_id,
            event_id,
            event_title,
            event_description,
            event_image,
            question,
            description,
            image,
            active,
            closed,
            maturity_reached,
            end_date,
            event_payload,
            market_payload,
            first_seen_at,
            last_seen_at,
            updated_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15, now()
        )
        ON CONFLICT (market_id) DO UPDATE
        SET
            event_id = EXCLUDED.event_id,
            event_title = EXCLUDED.event_title,
            event_description = EXCLUDED.event_description,
            event_image = EXCLUDED.event_image,
            question = EXCLUDED.question,
            description = EXCLUDED.description,
            image = EXCLUDED.image,
            active = EXCLUDED.active,
            closed = EXCLUDED.closed,
            maturity_reached = EXCLUDED.maturity_reached,
            end_date = EXCLUDED.end_date,
            event_payload = EXCLUDED.event_payload,
            market_payload = EXCLUDED.market_payload,
            last_seen_at = EXCLUDED.last_seen_at,
            updated_at = now()
        "#,
    )
    .bind(&row.market_id)
    .bind(&row.event_id)
    .bind(&row.event_title)
    .bind(&row.event_description)
    .bind(&row.event_image)
    .bind(&row.question)
    .bind(&row.description)
    .bind(&row.image)
    .bind(row.active)
    .bind(row.closed)
    .bind(row.maturity_reached)
    .bind(row.end_date)
    .bind(event_payload)
    .bind(market_payload)
    .bind(seen_at)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn mark_missing_markets_as_matured(
    pool: &PgPool,
    seen_at: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE warehouse.polymarket_markets
        SET
            maturity_reached = TRUE,
            active = FALSE,
            closed = TRUE,
            updated_at = now()
        WHERE last_seen_at < $1
          AND maturity_reached = FALSE
        "#,
    )
    .bind(seen_at)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as i64)
}
