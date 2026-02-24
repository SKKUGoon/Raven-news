# Raven News Agent Guide

This file defines required conventions for AI/code agents working in this repository.

## Primary Responsibilities

- Keep RSS and Polymarket ingestion reliable, observable, and backward compatible.
- Prefer small, targeted changes over broad refactors.
- Preserve existing CLI behavior unless explicitly requested.

## Required Update Checklist

When you add, remove, or change behavior, you must update all affected surfaces:

1. Code implementation in `src/` and related migrations in `migrations/`.
2. User-facing docs in `README.md` (commands, env vars, behavior).
3. Release automation in `.github/workflows/release.yml` when packaging, binary names, targets, or build commands are affected.

If any of the three are impacted and not updated, the task is incomplete.

## CLI Change Policy

When changing CLI commands/subcommands/options:

- Update command handling in `src/main.rs`.
- Update `README.md` CLI usage table and quick-start examples.
- Verify release workflow still builds/packages the expected binary in `.github/workflows/release.yml`.

## Database and Migration Policy

- Never modify an existing migration that may have already been applied in shared environments.
- Add a new migration for all schema changes.
- Keep schema names under `warehouse`.
- Prefer additive changes; avoid destructive migration patterns unless explicitly requested.

## Polymarket Ingestion Policy

- Default market discovery strategy: paginated events endpoint (`/events`) and extract nested markets.
- Keep backfill and incremental modes on a shared core pipeline, differing only in filtering strategy.
- Do not hard-delete markets; mark lifecycle transition via status fields (e.g., `maturity_reached`).
- Preserve raw payload columns for traceability when available.

## Rate Limit and Retry Policy

- Respect documented endpoint limits with deliberate pacing.
- Use bounded retries for transient errors (not infinite loops).
- Handle HTTP 429 with backoff before retrying.
- Keep request concurrency conservative unless explicitly required.

## Testing and Validation Expectations

Before finalizing changes:

- Run formatting (`cargo fmt`) for Rust edits.
- Run compile checks (`cargo check` or `SQLX_OFFLINE=true cargo check`).
- Run migrations when schema changes are introduced (`sqlx migrate run`) in an appropriate environment.
- If runtime verification is not possible, state exactly what remains to be validated.

## Documentation Quality Bar

- `README.md` must reflect current commands and environment variables.
- Add concise operational notes for new long-running jobs/schedulers.
- Keep examples copy-pasteable and consistent with actual binary/subcommand names.
