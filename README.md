![Rust Edition](https://img.shields.io/badge/Rust-Edition%202025-b7410e) ![Tokio](https://img.shields.io/badge/Tokio-1.48-blueviolet) ![PostgreSQL](https://img.shields.io/badge/PostgreSQL-17%2B-336791) ![SQLx](https://img.shields.io/badge/sqlx-0.8-blue)

# Raven News

Raven News is a Rust-based CLI and library for ingesting, normalizing, and storing RSS feeds from financial news providers such as Reuters, Bloomberg, and CoinDesk. It combines resilient fetchers, source-aware de-duplication, and an opinionated PostgreSQL schema that is ready for analytics.

## Highlights

- Unified CLI for RSS + Polymarket ingestion, including one-shot, scheduler, backfill, and stats commands.
- Source-specific RSS parsers built with `quick-xml`, each conforming to a shared `RssParser` trait.
- Stable `RssItem` identifiers generated with SHA-256 fingerprints to avoid duplicates across runs.
- PostgreSQL-backed persistence layer powered by `sqlx`, with dedicated warehouse tables for RSS and Polymarket markets.
- Async-ready foundation using `tokio`, structured logging via `tracing`, and integration tests with sample feeds.

## Developer Prerequisites

- Rust toolchain (`rustup` recommended).
- `cargo` (bundled with Rust).
- `sqlx-cli` for applying database migrations. (During development)
- PostgreSQL instance (local or remote).

## Non-Developer Prerequisites

- PostgreSQL instance (local or remote).
  - ```bash
    export DATABASE_URL="postgres://<id>:<password>@<host>:<port>/<dbname>"
    ```
- Your choice of multiple runners. `tmux` `nohup &` etc.
- Ready to go!

## Installation
1. **Download the release tarball:**
   ```bash
   VERSION=v0.1.3  # check the latest version
   wget https://github.com/skkugoon/RavenNews/releases/download/${VERSION}/raven-news-${VERSION}-x86_64-unknown-linux-gnu.tar.gz
   ```
2. **Extract the binaries:**
   ```bash
   tar -xzf raven-news-${VERSION}-x86_64-unknown-linux-gnu.tar.gz
   ```
3. **Install system-wide:**
   ```bash
   sudo mv raven-news /usr/local/bin
   ```
4. **Verify installation:**
   ```bash
   raven-news --help
   ```

## Quick Start

1. Install the Rust toolchain:
   ```bash
   rustup toolchain install stable
   ```
2. Install the SQLx CLI:
   ```bash
   cargo install sqlx-cli
   ```
3. Create a PostgreSQL database and export the connection string:
   ```bash
   export DATABASE_URL=postgres://postgres:password@localhost:5432/raven_news
   ```
4. Apply the migrations:
   ```bash
   sqlx migrate run
   ```
5. (Optional) Create a `.env` file to persist environment variables for `dotenvy`:
   ```bash
   cat <<'EOF' > .env
   DATABASE_URL=postgres://postgres:password@localhost:5432/raven_news
   RUST_LOG=info
   EOF
   ```
6. Verify the connection and insert RSS snapshot items:
   ```bash
   cargo run -- fetch-once
   ```
7. (Optional) Run Polymarket backfill (guarded by confirmation):
   ```bash
   cargo run -- backfill
   ```

## Configuration

- `DATABASE_URL` must be provided; `dotenvy` will automatically load a local `.env` file.
- Logging is handled by `tracing` with `EnvFilter`; set `RUST_LOG=debug` to increase verbosity.
- Modify the fetch cadence by editing `tokio::time::interval` in `src/ingest/mod.rs`.
- Polymarket pagination/rate-limit tunables:
  - `POLYMARKET_MAX_PAGES` (default `100`)
  - `POLYMARKET_PAGE_LIMIT` (default `100`, max `100`)
  - `POLYMARKET_REQUEST_DELAY_MS` (default `250`)
- Polymarket whitelist config lives in `polymarket_config.toml`:
  ```toml
  [whitelist]
  tag_slugs = ["economy", "trade"]
  ```

## CLI Usage

| Command | Purpose |
| --- | --- |
| `cargo run -- fetch-once` | Interactive selector (arrow keys): choose `polymarket` or `news items`, then run one-shot ingestion. |
| `cargo run -- run` | Interactive selector (arrow keys): choose `polymarket` or `news items`, then start the selected scheduler. |
| `cargo run -- backfill` | Backfill Polymarket markets broadly using the events endpoint pagination (shows confirmation prompt). |
| `cargo run -- stats` | Interactive selector: choose period first, then source (`polymarket` + distinct RSS sources from DB). |

## Polymarket Ingestion

- Source endpoint: `https://gamma-api.polymarket.com/events`.
- Primary strategy: retrieve markets via paginated events (events include nested markets).
- One-shot incremental sync is available through `fetch-once` by selecting `polymarket`.
- Backfill sync (`backfill`) uses the same core pipeline without active/open filtering for broader market coverage.
- Hourly sync is available through `run` by selecting `polymarket`.
- Event rows are stored in `warehouse.polymarket_events` and synced by `event_id`.
- `fetch-once` and `run` provide an interactive arrow-key selection between Polymarket and RSS ingestion.

## Parser Library

The CLI is backed by a library that you can embed elsewhere. Parsers live in `src/rss` and implement the shared `RssParser` trait. Example:

```rust
use raven_news::rss::{reuters::ReutersRssParser, RssParser};

let xml = reqwest::get("https://ir.thomsonreuters.com/rss/news-releases.xml?items=5")
    .await?
    .text()
    .await?;
let parser = ReutersRssParser;
let items = parser.parse(&xml)?;
```

Available parser modules:

- `rss::bloomberg` ingests wealth, economics, and markets feeds.
- `rss::coindesk` supports domain-tagged categories and multiple authors.
- `rss::reuters` handles financial, event, and SEC filing feeds.

Each parser defers to `RssItem::new`, which produces deterministic UUIDs by hashing the source, title, and publish timestamp.

## Database Layout

- Migration `100_create_warehouse_schema.sql` creates schema `warehouse` with table `rss_items`.
- Migration `104_normalize_polymarket_events_markets.sql` adds `warehouse.polymarket_events` and normalizes market/event fields.
- Migration `105_trim_polymarket_events_only.sql` trims Polymarket storage to event-only fields.
- Migration `106_drop_unused_polymarket_tables.sql` removes deprecated Polymarket market/outcome tables.
- `warehouse.rss_items` stores normalized RSS items with deterministic UUID keys.
- `warehouse.polymarket_events` stores event-level metadata: `event_id`, `event_title`, `active`, `closed`, `created_at`, `updated_at`.
- Database helpers in `src/db/stats.rs` expose RSS total, daily, and per-source counts for reporting.

## Testing

- Run unit and integration suites with:
  ```bash
  cargo test
  ```
- Database-aware tests require `DATABASE_URL` pointing to a writable PostgreSQL instance.
- The integration test at `tests/insert_process.rs` uses `tests/data/bloomberg_test.xml` to validate deduplication.

## Project Structure

```
├── migrations/                # SQLx migrations defining the warehouse schema
├── src/
│   ├── db/                    # PostgreSQL pool + insert & stats helpers
│   ├── ingest/                # Fetchers and scheduler loop
│   ├── rss/                   # Source-specific parsers implementing RssParser
│   ├── error.rs               # Domain error types
│   └── main.rs                # CLI entry point (clap-based)
├── tests/                     # Integration tests and fixtures
└── Cargo.toml
```

## Tests

- Unit and integration tests can be run with:
  ```bash
  cargo test
  ```
- Database-aware tests expect `DATABASE_URL` to be set (use `.env` or environment variables when invoking `cargo test`).

## Project Structure

```
├── migrations/                # SQLx migrations defining the warehouse schema
├── src/
│   ├── db/                    # PostgreSQL pool factory
│   ├── rss/
│   │   ├── bloomberg.rs       # Bloomberg RSS parser
│   │   ├── coindesk.rs        # CoinDesk RSS parser
│   │   └── reuters.rs         # Reuters RSS parser
│   └── main.rs                # Binary entry point (customize for your needs)
└── Cargo.toml
```

## Next Steps

- Add schedulers or jobs to poll feeds and persist results via the `db` module.
- Extend the schema and parsers to handle additional publishers or enrichments.
- Integrate alerting, search, or downstream analytics pipelines once ingestion is stable.