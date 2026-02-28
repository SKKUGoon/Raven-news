use crate::db::polymarket::{PolymarketEventRow, upsert_polymarket_event};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use thiserror::Error;
use tokio::select;
use tokio::time::{Duration, interval, sleep};
use tracing::{info, warn};

const POLYMARKET_EVENTS_API: &str = "https://gamma-api.polymarket.com/events";
const DEFAULT_PAGE_LIMIT: usize = 100;
const DEFAULT_MAX_PAGE_COUNT: usize = 100;
const DEFAULT_REQUEST_DELAY_MS: u64 = 250;
const MAX_RETRIES_ON_429: usize = 5;
const DEFAULT_RETRY_WAIT_MS: u64 = 1000;

#[derive(Debug)]
pub struct PolymarketSyncStats {
    pub events_fetched: usize,
    pub events_upserted: usize,
}

#[derive(Debug, Error)]
pub enum PolymarketIngestionError {
    #[error("Network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("JSON decoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML config parsing failed: {0}")]
    ConfigParse(#[from] toml::de::Error),
    #[error("Config read failed: {0}")]
    ConfigRead(#[from] std::io::Error),
    #[error("Polymarket config error: {0}")]
    Config(String),
    #[error("Polymarket API returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
}

#[derive(Clone, Debug)]
struct EventInfo {
    event_id: String,
    title: Option<String>,
    active: bool,
    closed: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub async fn fetch_and_sync_markets(
    pool: &PgPool,
) -> Result<PolymarketSyncStats, PolymarketIngestionError> {
    sync_events(pool, SyncMode::Incremental).await
}

pub async fn backfill_markets(
    pool: &PgPool,
) -> Result<PolymarketSyncStats, PolymarketIngestionError> {
    sync_events(pool, SyncMode::Backfill).await
}

#[derive(Clone, Copy, Debug)]
enum SyncMode {
    Incremental,
    Backfill,
}

async fn sync_events(
    pool: &PgPool,
    mode: SyncMode,
) -> Result<PolymarketSyncStats, PolymarketIngestionError> {
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(20))
        .build()?;
    let max_pages = max_page_count();
    let page_limit = page_limit();
    let delay_between_requests = request_delay();

    let mut events_fetched = 0usize;
    let mut events_upserted = 0usize;
    let mut fetched_open = 0usize;
    let mut fetched_closed = 0usize;

    let statuses: Vec<EventStatusQuery> = match mode {
        SyncMode::Incremental => vec![EventStatusQuery::ActiveOpen, EventStatusQuery::Closed],
        SyncMode::Backfill => vec![EventStatusQuery::Open, EventStatusQuery::Closed],
    };

    for status in statuses {
        for page_idx in 0..max_pages {
            let offset = page_idx * page_limit;
            let events = fetch_events_page(&client, status, offset, page_limit).await?;

            if events.is_empty() {
                break;
            }

            events_fetched += events.len();
            match status {
                EventStatusQuery::ActiveOpen | EventStatusQuery::Open => {
                    fetched_open += events.len()
                }
                EventStatusQuery::Closed => fetched_closed += events.len(),
            }
            let page_count = events.len();
            info!(
                "Fetched Polymarket events {:?} page {} (offset={}, events={})",
                status,
                page_idx + 1,
                offset,
                page_count
            );

            for event in events {
                let event_info = extract_event_info(&event);
                if event_info.event_id.is_empty() {
                    continue;
                }

                upsert_polymarket_event(
                    pool,
                    &PolymarketEventRow {
                        event_id: event_info.event_id,
                        event_title: event_info.title,
                        active: event_info.active,
                        closed: event_info.closed,
                        created_at: event_info.created_at,
                        updated_at: event_info.updated_at,
                    },
                )
                .await?;
                events_upserted += 1;
            }

            if page_count < page_limit {
                break;
            }

            sleep(delay_between_requests).await;
        }
    }

    info!(
        "Polymarket fetch summary: fetched_total={}, fetched_open={}, fetched_closed={}, upserted={}",
        events_fetched,
        fetched_open,
        fetched_closed,
        events_upserted
    );

    Ok(PolymarketSyncStats {
        events_fetched,
        events_upserted,
    })
}

pub async fn run_hourly_scheduler(pool: PgPool) {
    let mut ticker = interval(Duration::from_secs(30 * 60));

    info!("Polymarket scheduler started. Sync interval: 30 minutes.");
    loop {
        select! {
            _ = ticker.tick() => {
                info!("Running scheduled Polymarket sync...");
                match fetch_and_sync_markets(&pool).await {
                    Ok(stats) => {
                        info!(
                            "Polymarket sync complete: fetched={}, upserted={}",
                            stats.events_fetched,
                            stats.events_upserted
                        );
                    }
                    Err(err) => {
                        warn!("Polymarket sync failed: {err}");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received. Stopping Polymarket scheduler...");
                break;
            }
        }
    }
    info!("Polymarket scheduler stopped.");
}

async fn fetch_events_page(
    client: &reqwest::Client,
    status: EventStatusQuery,
    offset: usize,
    limit: usize,
) -> Result<Vec<Value>, PolymarketIngestionError> {
    let query = build_query(status, offset, limit);
    let mut attempts = 0usize;

    loop {
        let response = client
            .get(POLYMARKET_EVENTS_API)
            .query(&query)
            .send()
            .await?;
        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let body = response.text().await?;
            attempts += 1;
            if attempts > MAX_RETRIES_ON_429 {
                return Err(PolymarketIngestionError::HttpStatus {
                    status: status.as_u16(),
                    body,
                });
            }

            let wait = retry_wait_duration(attempts);
            warn!(
                "Rate-limited by Polymarket (429). Retrying in {}ms (attempt {}/{})",
                wait.as_millis(),
                attempts,
                MAX_RETRIES_ON_429
            );
            sleep(wait).await;
            continue;
        }

        if !status.is_success() {
            let body = response.text().await?;
            return Err(PolymarketIngestionError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let value: Value = response.json().await?;
        return Ok(events_from_response(value));
    }
}

#[derive(Clone, Copy, Debug)]
enum EventStatusQuery {
    ActiveOpen,
    Open,
    Closed,
}

fn build_query(
    status: EventStatusQuery,
    offset: usize,
    limit: usize,
) -> Vec<(&'static str, String)> {
    let mut query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
    match status {
        EventStatusQuery::ActiveOpen => {
            query.push(("active", "true".to_string()));
            query.push(("closed", "false".to_string()));
        }
        EventStatusQuery::Open => {
            query.push(("closed", "false".to_string()));
        }
        EventStatusQuery::Closed => {
            query.push(("closed", "true".to_string()));
        }
    }
    query
}

fn events_from_response(value: Value) -> Vec<Value> {
    if let Some(events) = value.as_array() {
        return events.to_vec();
    }

    if let Some(events) = value.get("events").and_then(Value::as_array) {
        return events.to_vec();
    }

    Vec::new()
}

fn extract_event_info(event: &Value) -> EventInfo {
    EventInfo {
        event_id: read_string(event, &["id", "eventId", "event_id"]).unwrap_or_default(),
        title: read_string(event, &["title", "name"]),
        active: event.get("active").and_then(Value::as_bool).unwrap_or(true),
        closed: event
            .get("closed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        created_at: read_datetime(event, &["createdAt", "creationDate"]).unwrap_or_else(Utc::now),
        updated_at: read_datetime(event, &["updatedAt"]).unwrap_or_else(Utc::now),
    }
}

fn read_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(raw) = value.get(*key) {
            if let Some(text) = raw.as_str() {
                return Some(text.to_string());
            }
            if let Some(num) = raw.as_i64() {
                return Some(num.to_string());
            }
            if let Some(num) = raw.as_u64() {
                return Some(num.to_string());
            }
            if let Some(num) = raw.as_f64() {
                return Some(num.to_string());
            }
        }
    }
    None
}

fn read_datetime(value: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    for key in keys {
        if let Some(raw) = value.get(*key) {
            if let Some(text) = raw.as_str() {
                if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
                    return Some(dt.with_timezone(&Utc));
                }
            }
            if let Some(epoch) = raw.as_i64() {
                if let Some(dt) = Utc.timestamp_opt(epoch, 0).single() {
                    return Some(dt);
                }
            }
        }
    }
    None
}

fn max_page_count() -> usize {
    std::env::var("POLYMARKET_MAX_PAGES")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .unwrap_or(DEFAULT_MAX_PAGE_COUNT)
}

fn page_limit() -> usize {
    std::env::var("POLYMARKET_PAGE_LIMIT")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|count| *count > 0 && *count <= DEFAULT_PAGE_LIMIT)
        .unwrap_or(DEFAULT_PAGE_LIMIT)
}

fn request_delay() -> Duration {
    let millis = std::env::var("POLYMARKET_REQUEST_DELAY_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .unwrap_or(DEFAULT_REQUEST_DELAY_MS);
    Duration::from_millis(millis)
}

fn retry_wait_duration(attempt: usize) -> Duration {
    let attempt_u64 = u64::try_from(attempt).unwrap_or(1);
    let multiplier = 2u64.saturating_pow(u32::try_from(attempt_u64.saturating_sub(1)).unwrap_or(0));
    Duration::from_millis(DEFAULT_RETRY_WAIT_MS.saturating_mul(multiplier))
}
