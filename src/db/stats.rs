use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[derive(Clone, Copy)]
pub enum StatsPeriod {
    AllTime,
    Today,
}

pub struct PolymarketVolumeSpike {
    pub event_id: String,
    pub event_title: Option<String>,
    pub captured_at: DateTime<Utc>,
    pub total_volume: Option<f64>,
    pub previous_total_volume: Option<f64>,
    pub volume_delta: f64,
    pub volume_pct: Option<f64>,
}

pub async fn count_total_rss_items(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint FROM warehouse.rss_items
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn count_daily_rss_items(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint FROM warehouse.rss_items
        WHERE published_at >= DATE_TRUNC('day', NOW())
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn count_source_rss_items(pool: &PgPool, source: &str) -> Result<i64, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint FROM warehouse.rss_items
        WHERE source = $1
        "#,
    )
    .bind(source)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

pub async fn list_distinct_rss_sources(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT source
        FROM warehouse.rss_items
        WHERE source IS NOT NULL AND source <> ''
        ORDER BY source
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn count_rss_items_by_period_and_source(
    pool: &PgPool,
    period: StatsPeriod,
    source: &str,
) -> Result<i64, sqlx::Error> {
    let query = match period {
        StatsPeriod::AllTime => {
            r#"
            SELECT COUNT(*)::bigint
            FROM warehouse.rss_items
            WHERE source = $1
            "#
        }
        StatsPeriod::Today => {
            r#"
            SELECT COUNT(*)::bigint
            FROM warehouse.rss_items
            WHERE source = $1
              AND published_at >= DATE_TRUNC('day', NOW())
            "#
        }
    };

    let count = sqlx::query_scalar::<_, i64>(query)
        .bind(source)
        .fetch_one(pool)
        .await?;

    Ok(count)
}

pub async fn count_polymarket_events_by_period(
    pool: &PgPool,
    period: StatsPeriod,
) -> Result<i64, sqlx::Error> {
    let query = match period {
        StatsPeriod::AllTime => {
            r#"
            SELECT COUNT(*)::bigint
            FROM warehouse.polymarket_events
            "#
        }
        StatsPeriod::Today => {
            r#"
            SELECT COUNT(*)::bigint
            FROM warehouse.polymarket_events
            WHERE created_at >= DATE_TRUNC('day', NOW())
            "#
        }
    };

    let count = sqlx::query_scalar::<_, i64>(query).fetch_one(pool).await?;
    Ok(count)
}

pub async fn list_polymarket_volume_spikes(
    pool: &PgPool,
    min_volume_delta: f64,
    min_volume_pct: f64,
    limit: i64,
) -> Result<Vec<PolymarketVolumeSpike>, sqlx::Error> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            Option<String>,
            DateTime<Utc>,
            Option<f64>,
            Option<f64>,
            f64,
            Option<f64>,
        ),
    >(
        r#"
        WITH paired AS (
            SELECT
                h.event_id,
                h.captured_at,
                h.total_volume,
                LAG(h.total_volume) OVER (
                    PARTITION BY h.event_id
                    ORDER BY h.captured_at
                ) AS previous_total_volume,
                ROW_NUMBER() OVER (
                    PARTITION BY h.event_id
                    ORDER BY h.captured_at DESC
                ) AS rn
            FROM warehouse.polymarket_event_metrics_history h
        )
        SELECT
            p.event_id,
            e.event_title,
            p.captured_at,
            p.total_volume,
            p.previous_total_volume,
            (COALESCE(p.total_volume, 0) - COALESCE(p.previous_total_volume, 0)) AS volume_delta,
            CASE
                WHEN COALESCE(p.previous_total_volume, 0) = 0 THEN NULL
                ELSE (COALESCE(p.total_volume, 0) - COALESCE(p.previous_total_volume, 0))
                     / p.previous_total_volume
            END AS volume_pct
        FROM paired p
        JOIN warehouse.polymarket_events e
          ON e.event_id = p.event_id
        WHERE p.rn = 1
          AND p.previous_total_volume IS NOT NULL
          AND (COALESCE(p.total_volume, 0) - COALESCE(p.previous_total_volume, 0)) >= $1
          AND COALESCE(
                CASE
                    WHEN p.previous_total_volume = 0 THEN NULL
                    ELSE (COALESCE(p.total_volume, 0) - COALESCE(p.previous_total_volume, 0))
                         / p.previous_total_volume
                END,
                0
              ) >= $2
        ORDER BY volume_delta DESC, p.captured_at DESC
        LIMIT $3
        "#,
    )
    .bind(min_volume_delta)
    .bind(min_volume_pct)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                event_id,
                event_title,
                captured_at,
                total_volume,
                previous_total_volume,
                volume_delta,
                volume_pct,
            )| PolymarketVolumeSpike {
                event_id,
                event_title,
                captured_at,
                total_volume,
                previous_total_volume,
                volume_delta,
                volume_pct,
            },
        )
        .collect())
}
