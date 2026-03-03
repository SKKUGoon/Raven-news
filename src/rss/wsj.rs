use crate::error::RssParseError;
use crate::rss::strip_cdata;
use crate::rss::{RssItem, RssParser, RssResult};
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsjRssItem {
    pub title: String,
    pub link: String,
    pub description: Option<String>,
    pub published_at: DateTime<Utc>,
    pub creators: Vec<String>,
}

impl WsjRssItem {
    pub fn into_rss_item(self) -> RssItem {
        RssItem::new(
            "wsj",
            self.title,
            self.link,
            self.description,
            Some(self.published_at),
        )
    }
}

pub struct WsjRssParser;

impl RssParser for WsjRssParser {
    fn parse(&self, xml: &str) -> RssResult<Vec<RssItem>> {
        let mut reader: Reader<&[u8]> = Reader::from_str(xml);

        let mut buf = Vec::new();
        let mut items = Vec::new();
        let mut in_item = false;

        let mut title: Option<String> = None;
        let mut link: Option<String> = None;
        let mut description: Option<String> = None;
        let mut published_at: Option<String> = None;
        let mut creators: Vec<String> = Vec::new();

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
                    b"dc:creator" if in_item => {
                        if let Some(creator) = reader
                            .read_text(QName(b"dc:creator"))
                            .ok()
                            .map(|s| strip_cdata(&s))
                        {
                            creators.push(creator);
                        }
                    }
                    _ => {}
                },
                Ok(Event::End(ref e)) if e.name().as_ref() == b"item" => {
                    in_item = false;

                    if let (Some(title), Some(link), Some(pubdate)) =
                        (title.take(), link.take(), published_at.take())
                    {
                        let parsed_dt = DateTime::parse_from_rfc2822(&pubdate)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());

                        let item = WsjRssItem {
                            title,
                            link,
                            description: description.take(),
                            published_at: parsed_dt,
                            creators: creators.clone(),
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
