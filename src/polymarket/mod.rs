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
    tag_slugs: Option<String>,
    tags: Option<String>,
    tag_labels: Option<String>,
    end_date: Option<DateTime<Utc>>,
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

    let statuses: Vec<EventStatusQuery> = match mode {
        // Open-only fetch keeps memory/time lower but does not observe open->closed transitions.
        SyncMode::Incremental => vec![EventStatusQuery::ActiveOpen],
        SyncMode::Backfill => vec![EventStatusQuery::Open],
    };

    for status in statuses {
        for page_idx in 0..max_pages {
            let offset = page_idx * page_limit;
            let events = fetch_events_page(&client, status, offset, page_limit).await?;

            if events.is_empty() {
                break;
            }

            events_fetched += events.len();
            fetched_open += events.len();
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
                        event_tag_slugs: event_info.tag_slugs,
                        event_tags: event_info.tags,
                        event_tag_labels: event_info.tag_labels,
                        end_date: event_info.end_date,
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
        "Polymarket fetch summary: fetched_total={}, fetched_open={}, upserted={}",
        events_fetched,
        fetched_open,
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
    let (tag_slugs, tags, tag_labels) = extract_event_tags(event);

    EventInfo {
        event_id: read_string(event, &["id", "eventId", "event_id"]).unwrap_or_default(),
        title: read_string(event, &["title", "name"]),
        tag_slugs,
        tags,
        tag_labels,
        end_date: read_datetime(event, &["endDate", "end_date"]),
        active: event.get("active").and_then(Value::as_bool).unwrap_or(true),
        closed: event
            .get("closed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        created_at: read_datetime(event, &["createdAt", "creationDate"]).unwrap_or_else(Utc::now),
        updated_at: read_datetime(event, &["updatedAt"]).unwrap_or_else(Utc::now),
    }
}

fn extract_event_tags(event: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let mut slugs: Vec<String> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut tags: Vec<String> = Vec::new();

    if let Some(tag_values) = event.get("tags").and_then(Value::as_array) {
        for tag in tag_values {
            let slug = read_string(tag, &["slug"]).map(|value| normalize_tag_value(&value));
            let label = read_string(tag, &["label"]).map(|value| normalize_tag_value(&value));

            if let Some(value) = slug.filter(|value| !value.is_empty()) {
                push_unique(&mut slugs, value.clone());
                push_unique(&mut tags, value);
            }

            if let Some(value) = label.filter(|value| !value.is_empty()) {
                push_unique(&mut labels, value.clone());
                push_unique(&mut tags, value);
            }
        }
    }

    (join_tag_values(slugs), join_tag_values(tags), join_tag_values(labels))
}

fn normalize_tag_value(value: &str) -> String {
    value.trim().to_lowercase()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn join_tag_values(values: Vec<String>) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
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
