# Ingest Agent Guide

## Purpose
`src/ingest` orchestrates RSS fetch -> parse -> insert. It is the runtime glue between parser modules and database writes.

## File In This Folder
- `mod.rs`: feed registry (`FEEDS`), one-shot fetch (`fetch_all_and_insert`), and scheduler (`run_scheduler`).

## How The Ingestion Section Should Be Crafted
### Feed Registry
- Add one `Feed` entry per URL in `FEEDS`.
- Set:
  - `name`: unique operational label (used in error context)
  - `url`: feed endpoint
  - `parser`: static parser implementing `RssParser`
  - `active`: gate inserts without removing fetch support

### Fetch Path
- `fetch_and_insert` should:
  1. Download XML (`reqwest::get`)
  2. Parse items via source parser
  3. Insert each parsed item through `insert_rss_item`

### Scheduler Path
- `run_scheduler` should keep interval-based polling and graceful Ctrl+C shutdown.

## Common Logic In Ingest
- Parser polymorphism through `&'static dyn RssParser`.
- Source-specific parsing is delegated to `src/rss/*`; ingestion remains source-agnostic.
- Fail fast by feed: errors are wrapped with `Feed '<name>' failed: ...`.
- Idempotency relies on DB `ON CONFLICT (id) DO NOTHING` in `insert_rss_item`.

## Rules When Adding A Feed
1. Reuse existing parser when schema matches; create new parser only when needed.
2. Keep `FEEDS` count in sync with array length.
3. Preserve deterministic behavior: avoid side effects in parser and ingestion loops.

## Validation Checklist
- `cargo check`
- Trigger `fetch-once` in CLI to verify end-to-end insert path.
