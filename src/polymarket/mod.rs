use crate::db::polymarket::{
    PolymarketMarketRow, mark_missing_markets_as_matured, upsert_polymarket_market,
};
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
    pub markets_processed: usize,
    pub matured_markets: i64,
}

#[derive(Clone, Copy, Debug)]
enum SyncMode {
    ActiveOpen,
    BackfillAll,
}

#[derive(Debug, Error)]
pub enum PolymarketIngestionError {
    #[error("Network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("JSON decoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Polymarket API returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
}

pub async fn fetch_and_sync_markets(
    pool: &PgPool,
) -> Result<PolymarketSyncStats, PolymarketIngestionError> {
    sync_markets(pool, SyncMode::ActiveOpen).await
}

pub async fn backfill_markets(
    pool: &PgPool,
) -> Result<PolymarketSyncStats, PolymarketIngestionError> {
    sync_markets(pool, SyncMode::BackfillAll).await
}

async fn sync_markets(
    pool: &PgPool,
    mode: SyncMode,
) -> Result<PolymarketSyncStats, PolymarketIngestionError> {
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(20))
        .build()?;
    let sync_started_at = Utc::now();
    let max_pages = max_page_count();
    let page_limit = page_limit();
    let delay_between_requests = request_delay();

    let mut events_fetched = 0usize;
    let mut markets_processed = 0usize;

    for page_idx in 0..max_pages {
        let offset = page_idx * page_limit;
        let events = fetch_events_page(&client, mode, offset, page_limit).await?;

        if events.is_empty() {
            break;
        }

        events_fetched += events.len();
        let page_count = events.len();
        info!(
            "Fetched Polymarket events page {} (offset={}, events={})",
            page_idx + 1,
            offset,
            page_count
        );

        for event in events {
            let event_payload = serde_json::to_string(&event)?;
            let event_info = extract_event_info(&event);

            for market in extract_markets(&event) {
                let row = build_market_row(&event_info, &market);
                if row.market_id.is_empty() {
                    continue;
                }

                let market_payload = serde_json::to_string(&market)?;
                upsert_polymarket_market(
                    pool,
                    &row,
                    &event_payload,
                    &market_payload,
                    sync_started_at,
                )
                .await?;
                markets_processed += 1;
            }
        }

        if page_count < page_limit {
            break;
        }

        sleep(delay_between_requests).await;
    }

    let matured_markets = match mode {
        SyncMode::ActiveOpen => mark_missing_markets_as_matured(pool, sync_started_at).await?,
        SyncMode::BackfillAll => 0,
    };

    Ok(PolymarketSyncStats {
        events_fetched,
        markets_processed,
        matured_markets,
    })
}

pub async fn run_hourly_scheduler(pool: PgPool) {
    let mut ticker = interval(Duration::from_secs(60 * 60));

    info!("Polymarket scheduler started. Sync interval: 1 hour.");
    loop {
        select! {
            _ = ticker.tick() => {
                info!("Running scheduled Polymarket sync...");
                match fetch_and_sync_markets(&pool).await {
                    Ok(stats) => {
                        info!(
                            "Polymarket sync complete: events={}, markets={}, matured={}",
                            stats.events_fetched,
                            stats.markets_processed,
                            stats.matured_markets
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

#[derive(Clone, Debug)]
struct EventInfo {
    event_id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    image: Option<String>,
}

fn extract_event_info(event: &Value) -> EventInfo {
    EventInfo {
        event_id: read_string(event, &["id", "eventId", "event_id"]),
        title: read_string(event, &["title", "name"]),
        description: read_string(event, &["description"]),
        image: read_string(event, &["image", "icon"]),
    }
}

fn extract_markets(event: &Value) -> Vec<Value> {
    event
        .get("markets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn build_market_row(event: &EventInfo, market: &Value) -> PolymarketMarketRow {
    let market_id =
        read_string(market, &["id", "conditionId", "condition_id", "slug"]).unwrap_or_default();
    let active = market
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let closed = market
        .get("closed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    PolymarketMarketRow {
        market_id,
        event_id: event.event_id.clone(),
        event_title: event.title.clone(),
        event_description: event.description.clone(),
        event_image: event.image.clone(),
        question: read_string(market, &["question", "title", "name"]),
        description: read_string(market, &["description"]),
        image: read_string(market, &["image", "icon"]),
        active,
        closed,
        maturity_reached: closed || !active,
        end_date: read_datetime(
            market,
            &[
                "endDate",
                "end_date",
                "endTime",
                "end_time",
                "expirationDate",
            ],
        ),
    }
}

async fn fetch_events_page(
    client: &reqwest::Client,
    mode: SyncMode,
    offset: usize,
    limit: usize,
) -> Result<Vec<Value>, PolymarketIngestionError> {
    let query = build_query(mode, offset, limit);
    let mut attempts = 0usize;

    loop {
        let response = client
            .get(POLYMARKET_EVENTS_API)
            .query(&query)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
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
            return Err(PolymarketIngestionError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let value: Value = serde_json::from_str(&body)?;
        return Ok(events_from_response(value));
    }
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

fn build_query(mode: SyncMode, offset: usize, limit: usize) -> Vec<(&'static str, String)> {
    let mut query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];

    match mode {
        SyncMode::ActiveOpen => {
            query.push(("active", "true".to_string()));
            query.push(("closed", "false".to_string()));
        }
        SyncMode::BackfillAll => {}
    }

    query
}

fn retry_wait_duration(attempt: usize) -> Duration {
    let attempt_u64 = u64::try_from(attempt).unwrap_or(1);
    let multiplier = 2u64.saturating_pow(u32::try_from(attempt_u64.saturating_sub(1)).unwrap_or(0));
    Duration::from_millis(DEFAULT_RETRY_WAIT_MS.saturating_mul(multiplier))
}
