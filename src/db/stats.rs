use sqlx::PgPool;

#[derive(Clone, Copy)]
pub enum StatsPeriod {
    AllTime,
    Today,
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
