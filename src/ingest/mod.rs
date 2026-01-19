use crate::db::insert_rss_item;
use crate::error::RssIngestionError;
use crate::rss::{
    RssParser, bloomberg::BloombergRssParser, coindesk::CoindeskRssParser,
    politico::PoliticoRssParser, reuters::ReutersRssParser,
};
use sqlx::PgPool;
use tokio::select;
use tokio::time::{Duration, interval};
use tracing::info;

struct Feed {
    name: &'static str,
    url: &'static str,
    parser: &'static dyn RssParser,
    active: bool,
}

static BLOOMBERG: BloombergRssParser = BloombergRssParser;
static COINDESK: CoindeskRssParser = CoindeskRssParser;
static REUTERS: ReutersRssParser = ReutersRssParser;
static POLITICO: PoliticoRssParser = PoliticoRssParser;

const FEEDS: [Feed; 10] = [
    Feed {
        name: "bloomberg_wealth",
        url: "https://feeds.bloomberg.com/wealth/news.rss",
        parser: &BLOOMBERG,
        active: true,
    },
    Feed {
        name: "bloomberg_economics",
        url: "https://feeds.bloomberg.com/economics/news.rss",
        parser: &BLOOMBERG,
        active: true,
    },
    Feed {
        name: "bloomberg_markets",
        url: "https://feeds.bloomberg.com/markets/news.rss",
        parser: &BLOOMBERG,
        active: true,
    },
    Feed {
        name: "coindesk",
        url: "https://www.coindesk.com/arc/outboundfeeds/rss",
        parser: &COINDESK,
        active: true,
    },
    Feed {
        name: "reuters_financial",
        url: "https://ir.thomsonreuters.com/rss/news-releases.xml?items=15",
        parser: &REUTERS,
        active: false, // Reuters financial no longer offers public free RSS feeds
    },
    Feed {
        name: "reuters_events",
        url: "https://ir.thomsonreuters.com/rss/events.xml?items=15",
        parser: &REUTERS,
        active: false,
    },
    Feed {
        name: "reuters_secfilings",
        url: "https://ir.thomsonreuters.com/rss/sec-filings.xml?items=15",
        parser: &REUTERS,
        active: false,
    },
    Feed {
        name: "politico_congress",
        url: "https://rss.politico.com/congress.xml",
        parser: &POLITICO,
        active: true,
    },
    Feed {
        name: "politico_economy",
        url: "https://rss.politico.com/economy.xml",
        parser: &POLITICO,
        active: true,
    },
    Feed {
        name: "politico_politics",
        url: "https://rss.politico.com/politics-news.xml",
        parser: &POLITICO,
        active: true,
    },
];

async fn fetch_and_insert(pool: &PgPool, feed: &Feed) -> Result<(), RssIngestionError> {
    let xml = reqwest::get(feed.url).await?.text().await?;
    let items = feed.parser.parse(&xml)?;

    for item in &items {
        if feed.active {
            insert_rss_item(pool, item).await?;
        }
    }

    Ok(())
}

pub async fn fetch_all_and_insert(pool: &PgPool) -> Result<(), RssIngestionError> {
    for feed in &FEEDS {
        if let Err(err) = fetch_and_insert(pool, feed).await {
            return Err(RssIngestionError::Other(format!(
                "Feed '{}' failed: {}",
                feed.name, err
            )));
        }
    }

    Ok(())
}

pub async fn run_scheduler(pool: PgPool) {
    let mut ticker = interval(Duration::from_secs(60));

    info!("Ingestion scheduler started. Press Ctrl+C to stop.");
    loop {
        select! {
            _ = ticker.tick() => {
                info!("Running scheduled RSS fetch...");
                if let Err(e) = fetch_all_and_insert(&pool).await {
                    eprintln!("Error fetching RSS: {e}");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received. Stopping ingestion scheduler...");
                break;
            }
        }
    }
    info!("Ingestion scheduler stopped.");
}
