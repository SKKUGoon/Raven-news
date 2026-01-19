use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use raven_news::db::create_pg_pool;
use raven_news::db::stats::{count_daily_rss_items, count_source_rss_items, count_total_rss_items};
use raven_news::ingest::{fetch_all_and_insert, run_scheduler};
use sqlx::PgPool;
use tracing::info;
use tracing_subscriber::{EnvFilter, filter::Directive};

#[derive(Parser)]
#[command(name = "raven-news")]
#[command(about = "RSS ingestion CLI")]
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

    /// Show ingestion statistics
    Stats {
        #[command(subcommand)]
        category: StatsCategory,
    },
}

#[derive(Subcommand)]
enum StatsCategory {
    Total,
    Daily,
    Source { name: String },
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

    match cli.command {
        Commands::FetchOnce => handle_fetch_once(&pool).await,
        Commands::Run => {
            info!("Running continuous fetch");
            run_scheduler(pool).await;
        }
        Commands::Stats { category } => {
            info!("Fetching total RSS items statistics");
            match category {
                StatsCategory::Total => print_total_stats(&pool).await,
                StatsCategory::Daily => print_daily_stats(&pool).await,
                StatsCategory::Source { name } => print_source_stats(&pool, &name).await,
            }
        }
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

async fn print_total_stats(pool: &PgPool) {
    info!("Fetching total RSS items");
    match count_total_rss_items(pool).await {
        Ok(cnt) => println!("Total RSS items: {cnt}"),
        Err(e) => {
            eprintln!("Failed to fetch total RSS items: {e}");
            std::process::exit(1);
        }
    }
}

async fn print_daily_stats(pool: &PgPool) {
    info!("Fetching daily RSS items");
    match count_daily_rss_items(pool).await {
        Ok(cnt) => println!("Daily RSS items: {cnt}"),
        Err(e) => {
            eprintln!("Failed to fetch daily RSS items: {e}");
            std::process::exit(1);
        }
    }
}

async fn print_source_stats(pool: &PgPool, name: &str) {
    info!("Fetching RSS items for source: {name}");
    match count_source_rss_items(pool, name).await {
        Ok(cnt) => println!("RSS items for {name}: {cnt}"),
        Err(e) => {
            eprintln!("Failed to fetch RSS items for source {name}: {e}");
            std::process::exit(1);
        }
    }
}
