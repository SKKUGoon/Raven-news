# Polymarket Agent Guide

## Purpose
`src/polymarket` ingests event data from Polymarket's API and upserts normalized event rows into `warehouse.polymarket_events`.

## File In This Folder
- `mod.rs`: API client flow, pagination, retry/backoff, event extraction/normalization, scheduler.

## How Each Section Should Be Crafted
### Public Entry Points
- Keep public functions small and orchestration-focused:
  - `fetch_and_sync_markets`: incremental mode
  - `backfill_markets`: broader mode
  - `run_hourly_scheduler`: recurring sync loop

### Sync Core
- `sync_events` should own:
  - client setup (timeouts)
  - page loop (`offset`, `limit`, `max pages`)
  - fetch + extract + DB upsert
  - summary stats

### HTTP Layer
- `fetch_events_page` should handle:
  - status checks
  - retries on 429 with exponential backoff
  - parsing both top-level array and `{ events: [] }` response forms

### Normalization Layer
- `extract_event_info` and helpers should:
  - support multiple candidate JSON key names
  - normalize tags (trim + lowercase + dedupe)
  - parse RFC3339 and epoch timestamps

## Common Logic In Polymarket
- Defensive parsing against schema drift (`id`/`eventId`/`event_id`, etc.).
- Deterministic tag serialization (`", "` joined strings).
- Parse and persist event-level interest metrics (`volume`, `volume24hr`, `openInterest`, `liquidity`) with numeric/string tolerance.
- Configurable fetch pressure via env vars:
  - `POLYMARKET_MAX_PAGES`
  - `POLYMARKET_PAGE_LIMIT`
  - `POLYMARKET_REQUEST_DELAY_MS`
- Scheduler should run on 30-minute boundaries (`:00`/`:30`, UTC by default).
- Sync stats are returned, and scheduler logs outcomes.

## When Modifying This Module
1. Keep retry and pagination logic separate from extraction helpers.
2. Maintain non-panicking behavior for malformed API payloads.
3. Preserve upsert semantics by sending complete `PolymarketEventRow` values.
4. Do not append metric history rows for closed events.

## Validation Checklist
- `cargo check`
- Run `fetch-once` (polymarket target) or `backfill` in a configured environment.
