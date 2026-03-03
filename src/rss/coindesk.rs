use crate::error::RssParseError;
use crate::rss::strip_cdata;
use crate::rss::{RssItem, RssParser, RssResult};
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoindeskRssItem {
    pub title: String,
    pub link: String,
    pub description: Option<String>,
    pub published_at: DateTime<Utc>,
    pub creators: Vec<String>,
    pub categories: Vec<CoindeskCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoindeskCategory {
    pub domain: Option<String>,
    pub name: String,
}

impl CoindeskRssItem {
    pub fn into_rss_item(self) -> RssItem {
        RssItem::new(
            "coindesk",
            self.title,
            self.link,
            self.description,
            Some(self.published_at),
        )
    }
}

pub struct CoindeskRssParser;

impl RssParser for CoindeskRssParser {
    fn parse(&self, xml: &str) -> RssResult<Vec<RssItem>> {
        let mut reader: Reader<&[u8]> = Reader::from_str(xml);

        let mut buf = Vec::new();
        let mut items = Vec::new();
        let mut in_item = false;

        // fields
        let mut title: Option<String> = None;
        let mut link: Option<String> = None;
        let mut description: Option<String> = None;
        let mut published_at: Option<String> = None;
        let mut creators: Vec<String> = Vec::new();
        let mut categories: Vec<CoindeskCategory> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"item" => {
                        in_item = true;
                        title = None;
                        link = None;
                        description = None;
                        published_at = None;
                        creators.clear();
                        categories.clear();
                    }
                    b"title" if in_item => {
                        title = reader
                            .read_text(QName(b"title"))
                            .ok()
                            .map(|s| strip_cdata(&s));
                    }
                    b"link" if in_item => {
                        link = reader
                            .read_text(QName(b"link"))
                            .ok()
                            .map(|s| s.into_owned());
                    }
                    b"description" if in_item => {
                        description = reader
                            .read_text(QName(b"description"))
                            .ok()
                            .map(|s| strip_cdata(&s));
                    }
                    b"pubDate" if in_item => {
                        published_at = reader
                            .read_text(QName(b"pubDate"))
                            .ok()
                            .map(|s| s.into_owned());
                    }
                    b"creator" if in_item => {
                        if let Some(creator) = reader
                            .read_text(QName(b"creator"))
                            .ok()
                            .map(|s| s.into_owned())
                        {
                            creators.push(creator);
                        }
                    }
                    b"category" if in_item => {
                        let mut domain: Option<String> = None;

                        for attr in e.attributes().flatten() {
                            if attr.key.into_inner() == b"domain" {
                                domain =
                                    Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
                            }
                        }

                        if let Ok(txt) = reader.read_text(QName(b"category")) {
                            let name = strip_cdata(&txt);
                            categories.push(CoindeskCategory { domain, name });
                        }
                    }
                    _ => {}
                },

                Ok(Event::End(ref e)) if e.name().as_ref() == b"item" => {
                    in_item = false;

                    if let (Some(title), Some(link), Some(pub_date_str)) =
                        (title.take(), link.take(), published_at.take())
                    {
                        let published_at = DateTime::parse_from_rfc2822(&pub_date_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());

                        let item = CoindeskRssItem {
                            title,
                            link,
                            description: description.take(),
                            published_at,
                            creators: creators.clone(),
                            categories: categories.clone(),
                        };

                        items.push(item.into_rss_item());
                    }
                }

                Ok(Event::Eof) => break,
                Err(e) => return Err(RssParseError::Xml(e.to_string())),
                _ => {}
            }
        }

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
    fn test_coindesk_rss() {
        let xml = fetch_xml("https://www.coindesk.com/arc/outboundfeeds/rss");
        let parser = CoindeskRssParser;
        let items = parser.parse(&xml).expect("Failed to parse XML");

        println!("Coindesk RSS: {} items", items.len());

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
