use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub struct PolymarketEventRow {
    pub event_id: String,
    pub event_title: Option<String>,
    pub event_tag_slugs: Option<String>,
    pub event_tags: Option<String>,
    pub event_tag_labels: Option<String>,
    pub end_date: Option<DateTime<Utc>>,
    pub total_volume: Option<f64>,
    pub volume_24h: Option<f64>,
    pub open_interest: Option<f64>,
    pub liquidity: Option<f64>,
    pub active: bool,
    pub closed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn upsert_polymarket_event(
    pool: &PgPool,
    row: &PolymarketEventRow,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO warehouse.polymarket_events (
            event_id,
            event_title,
            event_tag_slugs,
            event_tags,
            event_tag_labels,
            end_date,
            total_volume,
            volume_24h,
            open_interest,
            liquidity,
            active,
            closed,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (event_id) DO UPDATE
        SET
            event_title = EXCLUDED.event_title,
            event_tag_slugs = EXCLUDED.event_tag_slugs,
            event_tags = EXCLUDED.event_tags,
            event_tag_labels = EXCLUDED.event_tag_labels,
            end_date = EXCLUDED.end_date,
            total_volume = EXCLUDED.total_volume,
            volume_24h = EXCLUDED.volume_24h,
            open_interest = EXCLUDED.open_interest,
            liquidity = EXCLUDED.liquidity,
            active = EXCLUDED.active,
            closed = EXCLUDED.closed,
            created_at = EXCLUDED.created_at,
            updated_at = EXCLUDED.updated_at
        WHERE warehouse.polymarket_events.closed = FALSE
        "#,
    )
    .bind(&row.event_id)
    .bind(&row.event_title)
    .bind(&row.event_tag_slugs)
    .bind(&row.event_tags)
    .bind(&row.event_tag_labels)
    .bind(row.end_date)
    .bind(row.total_volume)
    .bind(row.volume_24h)
    .bind(row.open_interest)
    .bind(row.liquidity)
    .bind(row.active)
    .bind(row.closed)
    .bind(row.created_at)
    .bind(row.updated_at)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn insert_polymarket_event_metrics_history(
    pool: &PgPool,
    row: &PolymarketEventRow,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO warehouse.polymarket_event_metrics_history (
            event_id,
            total_volume,
            volume_24h,
            open_interest,
            liquidity
        )
        SELECT
            $1,
            $2,
            $3,
            $4,
            $5
        WHERE EXISTS (
            SELECT 1
            FROM warehouse.polymarket_events e
            WHERE e.event_id = $1
              AND e.closed = FALSE
        )
        "#,
    )
    .bind(&row.event_id)
    .bind(row.total_volume)
    .bind(row.volume_24h)
    .bind(row.open_interest)
    .bind(row.liquidity)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
