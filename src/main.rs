use clap::{Parser, Subcommand};
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use dotenvy::dotenv;
use raven_news::db::stats::{
    StatsPeriod, count_polymarket_events_by_period, count_rss_items_by_period_and_source,
    list_distinct_rss_sources,
};
use raven_news::db::{create_pg_pool, get_connection_status};
use raven_news::ingest::{fetch_all_and_insert, run_scheduler};
use raven_news::polymarket::{backfill_markets, fetch_and_sync_markets, run_hourly_scheduler};
use sqlx::PgPool;
use tracing::info;
use tracing_subscriber::{EnvFilter, filter::Directive};

#[derive(Parser)]
#[command(name = "raven-news")]
#[command(about = "RSS ingestion CLI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
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

    match cli.command {
        Commands::FetchOnce => handle_fetch_once_with_choice(&pool).await,
        Commands::Run => handle_run_with_choice(pool).await,
        Commands::Backfill => handle_polymarket_backfill(&pool).await,
        Commands::Stats => handle_stats_interactive(&pool).await,
    };
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

async fn handle_fetch_once(pool: &PgPool) {
    info!("Running one-time fetch");
    if let Err(e) = fetch_all_and_insert(pool).await {
        eprintln!("Failed to fetch RSS feeds: {e}");
        std::process::exit(1);
    }
}

async fn handle_fetch_once_with_choice(pool: &PgPool) {
    match select_target("fetch-once") {
        IngestionTarget::Polymarket => handle_polymarket_fetch_once(pool).await,
        IngestionTarget::NewsItems => handle_fetch_once(pool).await,
    }
}

async fn handle_run_with_choice(pool: PgPool) {
    match select_target("run") {
        IngestionTarget::Polymarket => {
            info!("Running Polymarket 30-minute sync");
            run_hourly_scheduler(pool).await;
        }
        IngestionTarget::NewsItems => {
            info!("Running continuous RSS fetch");
            run_scheduler(pool).await;
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
    let period = select_period();
    let (label, count) = match select_source(pool).await {
        StatsSource::Polymarket => {
            let count = count_polymarket_events_by_period(pool, period).await;
            ("polymarket".to_string(), count)
        }
        StatsSource::Rss(source) => {
            let count = count_rss_items_by_period_and_source(pool, period, &source).await;
            (source, count)
        }
    };

    match count {
        Ok(value) => {
            println!(
                "Stats | period={} | source={} | count={}",
                period_label(period),
                label,
                value
            );
        }
        Err(err) => {
            eprintln!("Failed to fetch stats: {err}");
            std::process::exit(1);
        }
    }
}

#[derive(Clone)]
enum StatsSource {
    Polymarket,
    Rss(String),
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

fn period_label(period: StatsPeriod) -> &'static str {
    match period {
        StatsPeriod::AllTime => "all-time",
        StatsPeriod::Today => "today",
    }
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
