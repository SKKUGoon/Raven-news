use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub struct PolymarketEventRow {
    pub event_id: String,
    pub event_title: Option<String>,
    pub event_tag_slugs: Option<String>,
    pub event_tags: Option<String>,
    pub event_tag_labels: Option<String>,
    pub end_date: Option<DateTime<Utc>>,
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
            active,
            closed,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (event_id) DO UPDATE
        SET
            event_title = EXCLUDED.event_title,
            event_tag_slugs = EXCLUDED.event_tag_slugs,
            event_tags = EXCLUDED.event_tags,
            event_tag_labels = EXCLUDED.event_tag_labels,
            end_date = EXCLUDED.end_date,
            active = EXCLUDED.active,
            closed = EXCLUDED.closed,
            created_at = EXCLUDED.created_at,
            updated_at = EXCLUDED.updated_at
        "#,
    )
    .bind(&row.event_id)
    .bind(&row.event_title)
    .bind(&row.event_tag_slugs)
    .bind(&row.event_tags)
    .bind(&row.event_tag_labels)
    .bind(row.end_date)
    .bind(row.active)
    .bind(row.closed)
    .bind(row.created_at)
    .bind(row.updated_at)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
