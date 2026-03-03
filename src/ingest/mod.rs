use crate::db::insert_rss_item;
use crate::error::RssIngestionError;
use crate::rss::{
    RssParser, bloomberg::BloombergRssParser, coindesk::CoindeskRssParser,
    politico::PoliticoRssParser, wsj::WsjRssParser,
};
use sqlx::PgPool;
use std::collections::HashSet;
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
static POLITICO: PoliticoRssParser = PoliticoRssParser;
static WSJ: WsjRssParser = WsjRssParser;

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
    Feed {
        name: "wsj_world",
        url: "https://feeds.content.dowjones.io/public/rss/RSSWorldNews",
        parser: &WSJ,
        active: true,
    },
    Feed {
        name: "wsj_politics",
        url: "https://feeds.content.dowjones.io/public/rss/socialpoliticsfeed",
        parser: &WSJ,
        active: true,
    },
    Feed {
        name: "wsj_markets",
        url: "https://feeds.content.dowjones.io/public/rss/RSSMarketsMain",
        parser: &WSJ,
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

fn active_feeds() -> Vec<&'static Feed> {
    FEEDS.iter().filter(|feed| feed.active).collect()
}

fn resolve_selected_active_feeds(
    selected_feed_names: &[String],
) -> Result<Vec<&'static Feed>, RssIngestionError> {
    let selected: HashSet<&str> = selected_feed_names.iter().map(String::as_str).collect();
    let feeds: Vec<&'static Feed> = FEEDS
        .iter()
        .filter(|feed| feed.active && selected.contains(feed.name))
        .collect();

    if feeds.is_empty() {
        return Err(RssIngestionError::Other(
            "No active RSS feeds selected".to_string(),
        ));
    }

    Ok(feeds)
}

async fn fetch_feeds(pool: &PgPool, feeds: &[&Feed]) -> Result<(), RssIngestionError> {
    for feed in feeds {
        if let Err(err) = fetch_and_insert(pool, feed).await {
            return Err(RssIngestionError::Other(format!(
                "Feed '{}' failed: {}",
                feed.name, err
            )));
        }
    }

    Ok(())
}

pub fn list_active_feed_names() -> Vec<&'static str> {
    FEEDS
        .iter()
        .filter(|feed| feed.active)
        .map(|feed| feed.name)
        .collect()
}

pub async fn fetch_selected_and_insert(
    pool: &PgPool,
    selected_feed_names: &[String],
) -> Result<(), RssIngestionError> {
    let feeds = resolve_selected_active_feeds(selected_feed_names)?;
    fetch_feeds(pool, &feeds).await
}

pub async fn fetch_all_and_insert(pool: &PgPool) -> Result<(), RssIngestionError> {
    let feeds = active_feeds();
    fetch_feeds(pool, &feeds).await
}

pub async fn run_scheduler(pool: PgPool) {
    let feeds = active_feeds();
    run_scheduler_for_feeds(pool, feeds).await;
}

pub async fn run_scheduler_for_selected(pool: PgPool, selected_feed_names: &[String]) {
    match resolve_selected_active_feeds(selected_feed_names) {
        Ok(feeds) => run_scheduler_for_feeds(pool, feeds).await,
        Err(err) => eprintln!("Error selecting RSS feeds: {err}"),
    }
}

async fn run_scheduler_for_feeds(pool: PgPool, feeds: Vec<&'static Feed>) {
    let mut ticker = interval(Duration::from_secs(60));

    info!(
        "Ingestion scheduler started for {} feed(s). Press Ctrl+C to stop.",
        feeds.len()
    );
    loop {
        select! {
            _ = ticker.tick() => {
                info!("Running scheduled RSS fetch...");
                if let Err(e) = fetch_feeds(&pool, &feeds).await {
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
