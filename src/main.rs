use clap::{Parser, Subcommand};
use dialoguer::{Confirm, MultiSelect, Select, theme::ColorfulTheme};
use dotenvy::dotenv;
use raven_news::db::stats::{
    StatsPeriod, count_polymarket_events_by_period, count_rss_items_by_period_and_source,
    list_distinct_rss_sources, list_polymarket_volume_spikes,
};
use raven_news::db::{create_pg_pool, get_connection_status};
use raven_news::ingest::{
    fetch_selected_and_insert, list_active_feed_names, run_scheduler_for_selected,
};
use raven_news::polymarket::{backfill_markets, fetch_and_sync_markets, run_hourly_scheduler};
use sqlx::PgPool;
use tracing::info;
use tracing_subscriber::{EnvFilter, filter::Directive};

const DEFAULT_SPIKE_MIN_VOLUME_DELTA: f64 = 10_000.0;
const DEFAULT_SPIKE_MIN_VOLUME_PCT: f64 = 0.5;
const DEFAULT_SPIKE_LIMIT: i64 = 20;

#[derive(Parser)]
#[command(name = "raven-news")]
#[command(about = "RSS ingestion CLI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Clone, Copy)]
enum Commands {
    /// Fetch RSS feeds one time and insert into DB (force snapshot)
    FetchOnce,

    /// Run continuous ingestion loop (every 60 seconds)
    Run,

    /// Backfill open Polymarket events with confirmation
    Backfill,

    /// Show ingestion statistics (interactive period/source selector)
    Stats,
}

impl Commands {
    fn label(self) -> &'static str {
        match self {
            Commands::FetchOnce => "fetch-once",
            Commands::Run => "run",
            Commands::Backfill => "backfill",
            Commands::Stats => "stats",
        }
    }
}

#[derive(Clone, Copy)]
enum IngestionTarget {
    Polymarket,
    NewsItems,
}

// CLI entry point
#[tokio::main]
async fn main() {
    dotenv().ok();
    init_tracing();
    info!("Starting Raven News CLI");

    let cli = Cli::parse();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = create_pg_pool(&database_url).await;
    print_startup_status(&pool).await;

    let command = cli.command.unwrap_or_else(select_command_interactive);

    match command {
        Commands::FetchOnce => handle_fetch_once_with_choice(&pool).await,
        Commands::Run => handle_run_with_choice(pool).await,
        Commands::Backfill => handle_polymarket_backfill(&pool).await,
        Commands::Stats => handle_stats_interactive(&pool).await,
    };
}

fn select_command_interactive() -> Commands {
    let commands = [
        Commands::FetchOnce,
        Commands::Run,
        Commands::Backfill,
        Commands::Stats,
    ];
    let labels: Vec<&str> = commands.iter().map(|cmd| cmd.label()).collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select command")
        .items(&labels)
        .default(0)
        .interact();

    match selection {
        Ok(index) => commands[index],
        Err(err) => {
            eprintln!("Command selection failed: {err}");
            std::process::exit(1);
        }
    }
}

fn init_tracing() {
    let directive = "info".parse::<Directive>().unwrap_or_else(|err| {
        eprintln!("Invalid log level directive: {err}");
        std::process::exit(1);
    });

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(directive))
        .init();
}

async fn handle_fetch_once_with_choice(pool: &PgPool) {
    match select_target("fetch-once") {
        IngestionTarget::Polymarket => handle_polymarket_fetch_once(pool).await,
        IngestionTarget::NewsItems => {
            let selected_sources = select_rss_sources();
            if selected_sources.is_empty() {
                println!("No RSS sources selected. Nothing to run.");
                return;
            }

            if let Err(e) = fetch_selected_and_insert(pool, &selected_sources).await {
                eprintln!("Failed to fetch selected RSS feeds: {e}");
                std::process::exit(1);
            }
        }
    }
}

async fn handle_run_with_choice(pool: PgPool) {
    match select_target("run") {
        IngestionTarget::Polymarket => {
            info!("Running Polymarket 30-minute sync");
            run_hourly_scheduler(pool).await;
        }
        IngestionTarget::NewsItems => {
            let selected_sources = select_rss_sources();
            if selected_sources.is_empty() {
                println!("No RSS sources selected. Nothing to run.");
                return;
            }

            info!("Running continuous RSS fetch");
            run_scheduler_for_selected(pool, &selected_sources).await;
        }
    }
}

fn select_target(command: &str) -> IngestionTarget {
    let options = ["polymarket", "news items"];
    let prompt = format!("Select target for `{command}`");

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(options)
        .default(0)
        .interact();

    match selection {
        Ok(0) => IngestionTarget::Polymarket,
        Ok(1) => IngestionTarget::NewsItems,
        Ok(_) => {
            eprintln!("Invalid selection.");
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!(
                "Interactive selection failed ({err}). Run in an interactive terminal for `fetch-once` and `run`."
            );
            std::process::exit(1);
        }
    }
}

fn select_rss_sources() -> Vec<String> {
    let sources = list_active_feed_names();
    if sources.is_empty() {
        return Vec::new();
    }

    let defaults = vec![true; sources.len()];

    let selection = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select RSS sources (space to toggle, enter to confirm)")
        .items(&sources)
        .defaults(&defaults)
        .interact();

    match selection {
        Ok(indices) => indices
            .into_iter()
            .map(|idx| sources[idx].to_string())
            .collect(),
        Err(err) => {
            eprintln!("RSS source selection failed: {err}");
            std::process::exit(1);
        }
    }
}

async fn handle_polymarket_fetch_once(pool: &PgPool) {
    info!("Running one-time Polymarket market sync");
    match fetch_and_sync_markets(pool).await {
        Ok(stats) => {
            println!(
                "Polymarket sync complete: fetched={}, upserted={}",
                stats.events_fetched, stats.events_upserted
            );
        }
        Err(e) => {
            eprintln!("Failed to sync Polymarket markets: {e}");
            std::process::exit(1);
        }
    }
}

async fn handle_polymarket_backfill(pool: &PgPool) {
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Polymarket backfill can take a long time and process many pages. Continue?")
        .default(false)
        .interact()
        .unwrap_or(false);

    if !confirmed {
        println!("Backfill cancelled.");
        return;
    }

    info!("Running Polymarket backfill sync");
    match backfill_markets(pool).await {
        Ok(stats) => {
            println!(
                "Polymarket backfill complete: fetched={}, upserted={}",
                stats.events_fetched, stats.events_upserted
            );
        }
        Err(e) => {
            eprintln!("Failed to backfill Polymarket markets: {e}");
            std::process::exit(1);
        }
    }
}

async fn handle_stats_interactive(pool: &PgPool) {
    match select_source(pool).await {
        StatsSource::Polymarket => handle_polymarket_stats(pool).await,
        StatsSource::Rss(source) => {
            let period = select_period();
            let count = count_rss_items_by_period_and_source(pool, period, &source).await;
            match count {
                Ok(value) => {
                    println!(
                        "Stats | period={} | source={} | count={}",
                        period_label(period),
                        source,
                        value
                    );
                }
                Err(err) => {
                    eprintln!("Failed to fetch stats: {err}");
                    std::process::exit(1);
                }
            }
        }
    }
}

#[derive(Clone)]
enum StatsSource {
    Polymarket,
    Rss(String),
}

#[derive(Clone, Copy)]
enum PolymarketStatsMode {
    EventCount,
    VolumeSpikes,
}

fn select_period() -> StatsPeriod {
    let options = ["all-time", "today"];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select period")
        .items(options)
        .default(0)
        .interact();

    match selection {
        Ok(0) => StatsPeriod::AllTime,
        Ok(1) => StatsPeriod::Today,
        Ok(_) => {
            eprintln!("Invalid period selection.");
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("Period selection failed: {err}");
            std::process::exit(1);
        }
    }
}

async fn select_source(pool: &PgPool) -> StatsSource {
    let mut sources = vec!["polymarket".to_string()];
    let db_sources = list_distinct_rss_sources(pool).await.unwrap_or_else(|err| {
        eprintln!("Failed to load RSS sources: {err}");
        std::process::exit(1);
    });

    for source in db_sources {
        if source != "polymarket" {
            sources.push(source);
        }
    }

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select source")
        .items(&sources)
        .default(0)
        .interact();

    match selection {
        Ok(idx) => {
            let selected = sources[idx].clone();
            if selected == "polymarket" {
                StatsSource::Polymarket
            } else {
                StatsSource::Rss(selected)
            }
        }
        Err(err) => {
            eprintln!("Source selection failed: {err}");
            std::process::exit(1);
        }
    }
}

async fn handle_polymarket_stats(pool: &PgPool) {
    match select_polymarket_stats_mode() {
        PolymarketStatsMode::EventCount => {
            let period = select_period();
            match count_polymarket_events_by_period(pool, period).await {
                Ok(value) => {
                    println!(
                        "Stats | period={} | source=polymarket | count={}",
                        period_label(period),
                        value
                    );
                }
                Err(err) => {
                    eprintln!("Failed to fetch stats: {err}");
                    std::process::exit(1);
                }
            }
        }
        PolymarketStatsMode::VolumeSpikes => {
            let min_delta = spike_min_volume_delta();
            let min_pct = spike_min_volume_pct();
            let limit = spike_limit();

            match list_polymarket_volume_spikes(pool, min_delta, min_pct, limit).await {
                Ok(spikes) => {
                    if spikes.is_empty() {
                        println!(
                            "No Polymarket volume spikes found (delta >= {:.0}, pct >= {:.0}%).",
                            min_delta,
                            min_pct * 100.0
                        );
                        return;
                    }

                    println!(
                        "Polymarket volume spikes (delta >= {:.0}, pct >= {:.0}%):",
                        min_delta,
                        min_pct * 100.0
                    );
                    for spike in spikes {
                        let title = spike
                            .event_title
                            .unwrap_or_else(|| "(untitled event)".to_string());
                        let current = spike.total_volume.unwrap_or(0.0);
                        let previous = spike.previous_total_volume.unwrap_or(0.0);
                        let pct = spike.volume_pct.map(|value| value * 100.0).unwrap_or(0.0);

                        println!(
                            "- {} | id={} | current={:.2} | prev={:.2} | delta={:.2} | pct={:.2}% | captured_at={}",
                            title,
                            spike.event_id,
                            current,
                            previous,
                            spike.volume_delta,
                            pct,
                            spike.captured_at
                        );
                    }
                }
                Err(err) => {
                    eprintln!("Failed to fetch Polymarket volume spikes: {err}");
                    std::process::exit(1);
                }
            }
        }
    }
}

fn select_polymarket_stats_mode() -> PolymarketStatsMode {
    let options = ["event-count", "volume-spikes"];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select Polymarket stats mode")
        .items(options)
        .default(0)
        .interact();

    match selection {
        Ok(0) => PolymarketStatsMode::EventCount,
        Ok(1) => PolymarketStatsMode::VolumeSpikes,
        Ok(_) => {
            eprintln!("Invalid Polymarket stats selection.");
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("Polymarket stats selection failed: {err}");
            std::process::exit(1);
        }
    }
}

fn period_label(period: StatsPeriod) -> &'static str {
    match period {
        StatsPeriod::AllTime => "all-time",
        StatsPeriod::Today => "today",
    }
}

fn spike_min_volume_delta() -> f64 {
    std::env::var("POLYMARKET_SPIKE_MIN_VOLUME_DELTA")
        .ok()
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|value| *value >= 0.0)
        .unwrap_or(DEFAULT_SPIKE_MIN_VOLUME_DELTA)
}

fn spike_min_volume_pct() -> f64 {
    std::env::var("POLYMARKET_SPIKE_MIN_VOLUME_PCT")
        .ok()
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|value| *value >= 0.0)
        .unwrap_or(DEFAULT_SPIKE_MIN_VOLUME_PCT)
}

fn spike_limit() -> i64 {
    std::env::var("POLYMARKET_SPIKE_LIMIT")
        .ok()
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SPIKE_LIMIT)
}

async fn print_startup_status(pool: &PgPool) {
    println!("raven-news v{}", env!("CARGO_PKG_VERSION"));
    match get_connection_status(pool).await {
        Ok(status) => {
            println!(
                "Connected to {}@{}:{} / {}",
                status.user, status.host, status.port, status.database
            );
        }
        Err(err) => {
            eprintln!("Connected, but failed to read DB status details: {err}");
        }
    }
}
