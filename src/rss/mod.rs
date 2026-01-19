pub mod bloomberg;
pub mod coindesk;
pub mod politico;
pub mod reuters;

use crate::error::RssParseError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct RssItem {
    pub id: Uuid,
    pub source: String,
    pub title: String,
    pub link: String,
    pub summary: Option<String>,
    pub published_at: DateTime<Utc>,
}

fn generate_rss_item_id(source: &str, title: &str, published_at: &DateTime<Utc>) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hasher.update(title.as_bytes());
    hasher.update(published_at.to_rfc3339().as_bytes());

    let hash = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    Uuid::from_bytes(bytes)
}

impl RssItem {
    pub fn new(
        source_name: impl Into<String>,
        title: impl Into<String>,
        link: impl Into<String>,
        summary: Option<String>,
        published_at: Option<DateTime<Utc>>,
    ) -> Self {
        let source_str = source_name.into();
        let title_str = title.into();
        let link_str = link.into();
        let published_at_dt = published_at.unwrap_or_else(Utc::now);

        let id = generate_rss_item_id(&source_str, &title_str, &published_at_dt);

        Self {
            id,
            source: source_str,
            title: title_str,
            link: link_str,
            summary,
            published_at: published_at_dt,
        }
    }
}

pub type RssResult<T> = Result<T, RssParseError>;

/// Trait: every RSS feed should implement this trait
pub trait RssParser {
    fn parse(&self, xml: &str) -> RssResult<Vec<RssItem>>;
}

// Utility functions
pub fn strip_cdata(text: &str) -> String {
    let t = text.trim();

    if t.starts_with("<![CDATA[") && t.ends_with("]]>") {
        t.trim_start_matches("<![CDATA[")
            .trim_end_matches("]]>")
            .to_string()
    } else {
        text.to_string()
    }
}
