use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub struct PolymarketEventRow {
    pub event_id: String,
    pub event_title: Option<String>,
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
            active,
            closed,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (event_id) DO UPDATE
        SET
            event_title = EXCLUDED.event_title,
            active = EXCLUDED.active,
            closed = EXCLUDED.closed,
            created_at = EXCLUDED.created_at,
            updated_at = EXCLUDED.updated_at
        "#,
    )
    .bind(&row.event_id)
    .bind(&row.event_title)
    .bind(row.active)
    .bind(row.closed)
    .bind(row.created_at)
    .bind(row.updated_at)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
