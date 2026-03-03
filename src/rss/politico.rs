use crate::error::RssParseError;
use crate::rss::strip_cdata;
use crate::rss::{RssItem, RssParser, RssResult};
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoliticoRssItem {
    pub title: String,
    pub link: String,
    pub summary_html: Option<String>,
    pub published_at: DateTime<Utc>,
    pub creators: Vec<String>,
    pub contributors: Vec<String>,
}

impl PoliticoRssItem {
    pub fn into_rss_item(self) -> RssItem {
        RssItem::new(
            "politico",
            self.title,
            self.link,
            self.summary_html,
            Some(self.published_at),
        )
    }
}

pub struct PoliticoRssParser;

impl PoliticoRssParser {
    fn parse_date(date_str: &str) -> Result<DateTime<Utc>, RssParseError> {
        DateTime::parse_from_rfc2822(date_str)
            .or_else(|_| DateTime::parse_from_rfc3339(date_str))
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|err| {
                RssParseError::InvalidDate(format!(
                    "Politico date parse failed: {date_str} ({err})"
                ))
            })
    }

    fn choose_summary(
        content_html: &Option<String>,
        description: &Option<String>,
    ) -> Option<String> {
        if let Some(content) = content_html {
            if !content.trim().is_empty() {
                return Some(content.clone());
            }
        }
        description.clone()
    }
}

impl RssParser for PoliticoRssParser {
    fn parse(&self, xml: &str) -> RssResult<Vec<RssItem>> {
        let mut reader: Reader<&[u8]> = Reader::from_str(xml);

        let mut buf = Vec::new();
        let mut items = Vec::new();
        let mut in_item = false;

        let mut title: Option<String> = None;
        let mut link: Option<String> = None;
        let mut description: Option<String> = None;
        let mut content_html: Option<String> = None;
        let mut pub_date: Option<String> = None;
        let mut modified_date: Option<String> = None;
        let mut creators: Vec<String> = Vec::new();
        let mut contributors: Vec<String> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"item" => {
                        in_item = true;
                        title = None;
                        link = None;
                        description = None;
                        content_html = None;
                        pub_date = None;
                        modified_date = None;
                        creators.clear();
                        contributors.clear();
                    }
                    b"title" if in_item => {
                        title = reader
                            .read_text(QName(b"title"))
                            .ok()
                            .map(|t| strip_cdata(&t));
                    }
                    b"link" if in_item => {
                        link = reader
                            .read_text(QName(b"link"))
                            .ok()
                            .map(|t| t.into_owned());
                    }
                    b"description" if in_item => {
                        description = reader
                            .read_text(QName(b"description"))
                            .ok()
                            .map(|t| strip_cdata(&t));
                    }
                    b"content:encoded" if in_item => {
                        content_html = reader
                            .read_text(QName(b"content:encoded"))
                            .ok()
                            .map(|t| strip_cdata(&t));
                    }
                    b"pubDate" if in_item => {
                        pub_date = reader
                            .read_text(QName(b"pubDate"))
                            .ok()
                            .map(|t| t.into_owned());
                    }
                    b"dcterms:modified" if in_item => {
                        modified_date = reader
                            .read_text(QName(b"dcterms:modified"))
                            .ok()
                            .map(|t| t.into_owned());
                    }
                    b"dc:creator" if in_item => {
                        if let Some(author) = reader
                            .read_text(QName(b"dc:creator"))
                            .ok()
                            .map(|t| strip_cdata(&t))
                        {
                            creators.push(author);
                        }
                    }
                    b"dc:contributor" if in_item => {
                        if let Some(contributor) = reader
                            .read_text(QName(b"dc:contributor"))
                            .ok()
                            .map(|t| strip_cdata(&t))
                        {
                            contributors.push(contributor);
                        }
                    }
                    _ => {}
                },
                Ok(Event::End(ref e)) if e.name().as_ref() == b"item" => {
                    if in_item {
                        in_item = false;

                        if let (Some(title), Some(link)) = (title.take(), link.take()) {
                            let date_candidate =
                                pub_date.as_ref().or(modified_date.as_ref()).cloned();

                            let published_at = if let Some(date_str) = date_candidate {
                                Self::parse_date(&date_str).unwrap_or_else(|_| Utc::now())
                            } else {
                                Utc::now()
                            };

                            let summary = Self::choose_summary(&content_html, &description);

                            let item = PoliticoRssItem {
                                title,
                                link,
                                summary_html: summary,
                                published_at,
                                creators: creators.clone(),
                                contributors: contributors.clone(),
                            };

                            items.push(item.into_rss_item());
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(RssParseError::Xml(e.to_string())),
                _ => {}
            }
            buf.clear();
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
    fn test_politico_congress_feed() {
        let xml = fetch_xml("https://rss.politico.com/congress.xml");
        let parser = PoliticoRssParser;
        let items = parser.parse(&xml).expect("Failed to parse Politico feed");

        println!("Politico Congress RSS: {} items", items.len());
        for item in items.iter().take(3) {
            println!("TITLE: {}", item.title);
            println!("LINK : {}", item.link);
        }
    }
}
