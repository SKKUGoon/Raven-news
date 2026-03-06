# DB Agent Guide

## Purpose
`src/db` is the persistence boundary. It owns PostgreSQL connection setup, insert/upsert queries, and stats queries.

## Files In This Folder
- `mod.rs`: pool creation, connection metadata, generic RSS insert.
- `polymarket.rs`: Polymarket event upsert shape + query.
- `stats.rs`: read-only aggregation queries used by CLI stats flow.

## How Each Section Should Be Crafted
### Connection Layer (`mod.rs`)
- Keep `create_pg_pool` simple and deterministic.
- Expose lightweight status helpers (`get_connection_status`) for startup diagnostics.

### Write Layer
- RSS writes: `insert_rss_item` should remain idempotent (`ON CONFLICT (id) DO NOTHING`).
- Polymarket writes: `upsert_polymarket_event` should update mutable fields on conflict.

### Read Layer (`stats.rs`)
- Keep query functions narrow and single-purpose.
- Use `StatsPeriod` enum + match-based SQL selection for clear period behavior.

## Common Logic Across DB Module
- Use `sqlx::query` / `query_scalar` with bound params (no string interpolation for user inputs).
- Return typed `Result<_, sqlx::Error>` and let caller decide user-facing handling.
- Match table contracts in `warehouse` schema:
  - `warehouse.rss_items`
  - `warehouse.polymarket_events`
  - `warehouse.polymarket_event_metrics_history`

## Rules For Future DB Changes
1. Keep data layer unaware of network/parsing concerns.
2. Prefer additive query helpers over overloading existing ones with flags.
3. If schema changes, update migrations first, then db query layer, then ingestion callers.
4. For Polymarket events, preserve closed-event write suppression semantics in upsert/history queries.

## Validation Checklist
- `cargo check`
- In DB-backed environments, run relevant tests in `src/db/mod.rs` and ingestion paths.
