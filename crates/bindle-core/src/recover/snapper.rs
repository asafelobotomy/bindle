//! snapper's list of the root config's snapshots (`snapper --jsonout -c root
//! list`), read leniently: snapper versions differ in which columns they
//! print, and only the number is needed to roll back.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub number: u32,
    /// single, pre or post.
    #[serde(default, rename = "type")]
    pub kind: String,
    /// A post snapshot's pre snapshot (snap-pac: before and after pacman).
    #[serde(default, rename = "pre-number", skip_serializing_if = "Option::is_none")]
    pub pre: Option<u32>,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub cleanup: String,
    /// What snap-pac records: the pacman command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userdata: Option<serde_json::Value>,
}

impl Snapshot {
    /// One line for a list: "12  2026-11-15 10:02  pacman -Syu (pre)".
    pub fn label(&self) -> String {
        let date = self.date.get(..16).unwrap_or(&self.date);
        let what = if self.description.is_empty() { "(no description)" } else { &self.description };
        let kind = match self.kind.as_str() {
            "pre" => " (before)",
            "post" => " (after)",
            _ => "",
        };
        format!("{}  {date}  {what}{kind}", self.number)
    }
}

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    root: Vec<Snapshot>,
}

/// The snapshots that can be rolled back to: every one but 0 (the running
/// system), newest first.
pub fn parse(json: &str) -> Result<Vec<Snapshot>, String> {
    let listing: Listing = serde_json::from_str(json).map_err(|e| format!("snapper's list isn't readable: {e}"))?;
    let mut out: Vec<Snapshot> = listing.root.into_iter().filter(|s| s.number != 0).collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.number));
    Ok(out)
}

/// One snapshot from its `info.xml` (`<N>/info.xml` beside `<N>/snapshot`):
/// how the live system lists a system that isn't running.
pub fn from_info(xml: &str) -> Option<Snapshot> {
    let tag = |name: &str| {
        let open = format!("<{name}>");
        let start = xml.find(&open)? + open.len();
        let end = xml[start..].find(&format!("</{name}>"))? + start;
        Some(xml[start..end].trim().replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">"))
    };
    Some(Snapshot {
        number: tag("num")?.parse().ok()?,
        kind: tag("type").unwrap_or_default(),
        pre: tag("pre_num").and_then(|n| n.parse().ok()),
        date: tag("date").unwrap_or_default(),
        description: tag("description").unwrap_or_default(),
        cleanup: tag("cleanup").unwrap_or_default(),
        userdata: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"{
      "root": [
        { "subvolume": "/", "number": 0, "default": false, "active": true, "type": "single", "date": "",
          "user": "root", "cleanup": "", "description": "current", "userdata": null },
        { "subvolume": "/", "number": 1, "type": "single", "date": "2026-11-14 09:00:01", "user": "root",
          "cleanup": "timeline", "description": "timeline", "userdata": null },
        { "subvolume": "/", "number": 2, "type": "pre", "date": "2026-11-15 10:02:33", "user": "root",
          "cleanup": "number", "description": "pacman -Syu", "userdata": null },
        { "subvolume": "/", "number": 3, "type": "post", "pre-number": 2, "date": "2026-11-15 10:04:10",
          "user": "root", "cleanup": "number", "description": "pacman -Syu", "userdata": null }
      ]
    }"#;

    #[test]
    fn newest_first_without_the_running_system() {
        let s = parse(LIST).unwrap();
        assert_eq!(s.iter().map(|s| s.number).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(s[0].pre, Some(2));
        assert_eq!(s[1].label(), "2  2026-11-15 10:02  pacman -Syu (before)");
        assert_eq!(parse(r#"{"root": []}"#).unwrap(), vec![]);
        assert!(parse("nope").is_err());
        // Only the number is required.
        assert_eq!(parse(r#"{"root": [{"number": 7}]}"#).unwrap()[0].label(), "7    (no description)");
    }

    #[test]
    fn info_xml() {
        let xml = "<?xml version=\"1.0\"?>\n<snapshot>\n  <type>post</type>\n  <num>5</num>\n  <pre_num>4</pre_num>\n  \
                   <date>2026-11-15 10:04:10</date>\n  <description>pacman -S a&amp;b</description>\n  <cleanup>number</cleanup>\n</snapshot>\n";
        let s = from_info(xml).unwrap();
        assert_eq!((s.number, s.pre, s.kind.as_str(), s.description.as_str()), (5, Some(4), "post", "pacman -S a&b"));
        assert!(from_info("<snapshot></snapshot>").is_none());
    }
}
