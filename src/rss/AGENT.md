# RSS Agent Guide

## Purpose
`src/rss` owns feed-specific parsing. Each source module converts raw XML into normalized `RssItem` values used by ingestion and DB layers.

## Files In This Folder
- `mod.rs`: shared `RssItem`, `RssParser` trait, ID generation, and parser utilities (`strip_cdata`).
- `bloomberg.rs`, `coindesk.rs`, `politico.rs`, `wsj.rs`: source-specific parsers.
- `seeking_alpha.rs`: currently present but not wired into ingestion.

## How A Parser Should Be Crafted
1. Define a source-specific struct (optional but recommended) with parsed fields.
2. Implement `into_rss_item()` and set a stable source name (`"bloomberg"`, `"wsj"`, etc.).
3. Implement `RssParser::parse(&self, xml: &str)`.
4. Parse inside `<item>` boundaries only.
5. Require at least `title`, `link`, and date for an item to be emitted.
6. Parse `pubDate` as RFC2822 (fall back to `Utc::now()` only if needed).
7. Clean text with `strip_cdata` where fields may contain CDATA.

## Common Logic Across RSS Parsers
- Use `quick_xml::Reader` + `read_event_into` loop.
- Track `in_item` state and reset per-item fields when `<item>` starts.
- Collect optional metadata (authors/categories) but only persist what maps to `RssItem`.
- Return `RssParseError::Xml` on XML parser failure.
- Normalize all output to `RssItem` so DB insert logic stays generic.

## Required Wiring Steps For New Source
1. Add `pub mod <source>;` in `src/rss/mod.rs`.
2. Add parser static + feed entry in `src/ingest/mod.rs`.
3. Keep `source` string stable; it drives dedupe IDs and stats filters.

## Validation Checklist
- `cargo check`
- Optional: run parser test(s) for that source.
- Confirm emitted `RssItem::new(...)` uses the intended source key.
