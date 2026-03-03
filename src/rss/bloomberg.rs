use crate::error::RssParseError;
use crate::rss::strip_cdata;
use crate::rss::{RssItem, RssParser, RssResult};
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloombergRssItem {
    pub title: String,
    pub link: String,
    pub summary_html: Option<String>,
    pub published_at: DateTime<Utc>,
    pub creator: Option<String>,
    pub categories: Vec<String>,
}

impl BloombergRssItem {
    pub fn into_rss_item(self) -> RssItem {
        RssItem::new(
            "bloomberg",
            self.title,
            self.link,
            self.summary_html,
            Some(self.published_at),
        )
    }
}

pub struct BloombergRssParser;

impl RssParser for BloombergRssParser {
    fn parse(&self, xml: &str) -> RssResult<Vec<RssItem>> {
        let mut reader = Reader::from_str(xml);

        let mut buf = Vec::new();
        let mut items = Vec::new();
        let mut in_item = false;

        // Fields for each <item>
        let mut title: Option<String> = None;
        let mut link: Option<String> = None;
        let mut description: Option<String> = None;
        let mut creator: Option<String> = None;
        let mut published_at: Option<String> = None;
        let mut categories: Vec<String> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"item" => {
                        in_item = true;
                        title = None;
                        link = None;
                        description = None;
                        creator = None;
                        published_at = None;
                        categories.clear();
                    }
                    b"title" => {
                        if in_item {
                            title = reader
                                .read_text(QName(b"title"))
                                .ok()
                                .map(|t| strip_cdata(&t));
                        }
                    }
                    b"link" => {
                        if in_item {
                            link = reader
                                .read_text(QName(b"link"))
                                .ok()
                                .map(|t| t.into_owned());
                        }
                    }
                    b"description" => {
                        if in_item {
                            description = reader
                                .read_text(QName(b"description"))
                                .ok()
                                .map(|t| strip_cdata(&t));
                        }
                    }
                    b"creator" => {
                        if in_item {
                            creator = reader
                                .read_text(QName(b"creator"))
                                .ok()
                                .map(|t| t.into_owned());
                        }
                    }
                    b"pubDate" => {
                        if in_item {
                            published_at = reader
                                .read_text(QName(b"pubDate"))
                                .ok()
                                .map(|t| t.into_owned());
                        }
                    }
                    b"category" => {
                        if in_item {
                            if let Ok(text) = reader.read_text(QName(b"category")) {
                                categories.push(text.into_owned());
                            }
                        }
                    }
                    _ => {}
                },
                Ok(Event::End(ref e)) => {
                    if e.name().as_ref() == b"item" && in_item {
                        in_item = false;

                        // Build BloombergRssItem
                        if let (Some(title), Some(link), Some(published_at_str)) =
                            (title.clone(), link.clone(), published_at.clone())
                        {
                            // Parse date safely
                            let published_dt = DateTime::parse_from_rfc2822(&published_at_str)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now());

                            let bloomberg_item = BloombergRssItem {
                                title,
                                link,
                                summary_html: description.clone(),
                                published_at: published_dt,
                                creator: creator.clone(),
                                categories: categories.clone(),
                            };

                            // Convert to generic RssItem
                            items.push(bloomberg_item.into_rss_item());
                        }
                    }
                }

                Ok(Event::Eof) => break,
                Err(e) => return Err(RssParseError::Xml(e.to_string())),
                _ => {}
            }
        }
        buf.clear();

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::blocking;

    fn fetch_xml(url: &str) -> String {
        blocking::get(url)
            .expect("HTTP GET failed")
            .text()
            .expect("Failed to read response body")
    }

    #[test]
    fn test_bloomberg_rss_wealth() {
        // https://feeds.bloomberg.com/wealth/news.rss
        let xml = fetch_xml("https://feeds.bloomberg.com/wealth/news.rss");
        let parser = BloombergRssParser;
        let items = parser.parse(&xml).expect("Failed to parse XML");

        // test print
        println!("Bloomberg Wealth RSS: {} items", items.len());

        // Print first 3 examples
        for item in items.iter().take(3) {
            println!("TITLE: {}", item.title);
            println!("LINK : {}", item.link);
            println!(
                "DESCRIPTION: {}",
                item.summary.as_ref().unwrap_or(&"N/A".to_string())
            );
            println!("------");
        }
    }

    #[test]
    fn test_bloomberg_rss_economics() {
        // https://feeds.bloomberg.com/economics/news.rss
        let xml = fetch_xml("https://feeds.bloomberg.com/economics/news.rss");
        let parser = BloombergRssParser;
        let items = parser.parse(&xml).expect("Failed to parse XML");

        // test print
        println!("Bloomberg Economics RSS: {} items", items.len());

        // Print first 3 examples
        for item in items.iter().take(3) {
            println!("TITLE: {}", item.title);
            println!("LINK : {}", item.link);
            println!(
                "DESCRIPTION: {}",
                item.summary.as_ref().unwrap_or(&"N/A".to_string())
            );
            println!("------");
        }
    }

    #[test]
    fn test_bloomberg_rss_markets() {
        // https://feeds.bloomberg.com/markets/news.rss
        let xml = fetch_xml("https://feeds.bloomberg.com/markets/news.rss");
        let parser = BloombergRssParser;
        let items = parser.parse(&xml).expect("Failed to parse XML");

        // test print
        println!("Bloomberg Markets RSS: {} items", items.len());

        // Print first 3 examples
        for item in items.iter().take(3) {
            println!("TITLE: {}", item.title);
            println!("LINK : {}", item.link);
            println!(
                "DESCRIPTION: {}",
                item.summary.as_ref().unwrap_or(&"N/A".to_string())
            );
            println!("------");
        }
    }
}
