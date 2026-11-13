//! The Arch Linux news feed: some upgrades need a manual step first, and
//! the news is where Arch says so.

use serde::{Deserialize, Serialize};

use crate::parse::day;

pub const FEED: &str = "https://archlinux.org/feeds/news/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    /// Days since 1970-01-01.
    pub day: i64,
}

impl NewsItem {
    /// Arch's titles say so when an upgrade needs a hand ("… requires manual
    /// intervention", "… needs manual intervention").
    pub fn manual(&self) -> bool {
        let t = self.title.to_lowercase();
        t.contains("manual intervention") || t.contains("requires manual") || t.contains("needs manual")
    }
}

/// Items of an RSS 2.0 feed. Only what the news feed uses: `<item>` with
/// `<title>`, `<link>` and `<pubDate>`; items without a readable date are
/// left out.
pub fn parse(xml: &str) -> Vec<NewsItem> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<item>") {
        let after = &rest[start + 6..];
        let end = after.find("</item>").unwrap_or(after.len());
        let item = &after[..end];
        rest = &after[end..];
        let (Some(title), Some(date)) = (tag(item, "title"), tag(item, "pubDate").and_then(|d| rfc2822_day(&d))) else {
            continue;
        };
        out.push(NewsItem { title, link: tag(item, "link").unwrap_or_default(), day: date });
    }
    out
}

/// News since `since` (inclusive: an upgrade that day may have come before
/// the post). Without a known last upgrade, the last `fallback_days`.
pub fn since(items: &[NewsItem], since: Option<i64>, today: i64, fallback_days: i64) -> Vec<NewsItem> {
    let from = since.unwrap_or(today - fallback_days);
    items.iter().filter(|i| i.day >= from).cloned().collect()
}

fn tag(item: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let s = item.find(&open)? + open.len();
    let e = s + item[s..].find(&close)?;
    let raw = item[s..e].trim();
    let raw = raw.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")).unwrap_or(raw);
    Some(unescape(raw.trim()))
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `Sat, 01 Nov 2026 12:34:56 +0000` → day number (the date as written;
/// a day's precision is all the comparison needs).
pub fn rfc2822_day(s: &str) -> Option<i64> {
    let s = s.split_once(',').map_or(s, |(_, r)| r);
    let mut w = s.split_whitespace();
    let d: u32 = w.next()?.parse().ok()?;
    let m = match w.next()?.get(..3)?.to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    let y: i64 = w.next()?.parse().ok()?;
    (1..=31).contains(&d).then(|| day(y, m, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0"><channel><title>Arch Linux: Recent news updates</title>
<item><title>Glibc 2.43 update requires manual intervention</title>
<link>https://archlinux.org/news/glibc-243/</link>
<description>&lt;p&gt;Run this first&lt;/p&gt;</description>
<pubDate>Tue, 11 Nov 2026 10:00:00 +0000</pubDate></item>
<item><title>Valkey &amp; friends move to [extra]</title>
<link>https://archlinux.org/news/valkey/</link>
<pubDate>Mon, 20 Oct 2026 08:00:00 +0000</pubDate></item>
<item><title>No date</title></item>
</channel></rss>"#;

    #[test]
    fn feed_items_parse() {
        let items = parse(FEED_XML);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Glibc 2.43 update requires manual intervention");
        assert_eq!(items[0].link, "https://archlinux.org/news/glibc-243/");
        assert_eq!(items[0].day, day(2026, 11, 11));
        assert_eq!(items[1].title, "Valkey & friends move to [extra]");
        assert!(items[0].manual() && !items[1].manual());
    }

    #[test]
    fn only_news_since_the_last_upgrade() {
        let items = parse(FEED_XML);
        let new = since(&items, Some(day(2026, 11, 1)), day(2026, 11, 13), 30);
        assert_eq!(new.len(), 1);
        assert_eq!(since(&items, Some(day(2026, 11, 11)), day(2026, 11, 13), 30).len(), 1, "same day counts");
        assert_eq!(since(&items, None, day(2026, 11, 13), 30).len(), 2);
        assert!(since(&items, None, day(2026, 11, 13), 1).is_empty());
    }

    #[test]
    fn dates_without_weekday_and_bad_dates() {
        assert_eq!(rfc2822_day("01 Nov 2026 00:00:00 GMT"), Some(day(2026, 11, 1)));
        assert_eq!(rfc2822_day("Sat, 41 Nov 2026"), None);
        assert_eq!(rfc2822_day("yesterday"), None);
    }
}
