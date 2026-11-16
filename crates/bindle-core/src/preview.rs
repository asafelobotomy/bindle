//! The plan document every `--dry-run --json` prints: what uQ Settings' confirm
//! sheet shows before an action (docs/settings-pages.md, "Progress and
//! previews"). One document, so the window and the command line say the same.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Change {
    pub group: String,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Left out: unknown. `false`: it can't be put back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversible: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlanDoc {
    pub summary: String,
    pub changes: Vec<Change>,
    pub would_refuse: bool,
    pub reason: String,
}

impl PlanDoc {
    pub fn change(&mut self, group: &str, label: impl Into<String>, detail: impl Into<String>, rev: Option<bool>) {
        self.changes.push(Change { group: group.into(), label: label.into(), detail: detail.into(), reversible: rev });
    }

    pub fn refuse(mut self, reason: impl Into<String>) -> Self {
        self.would_refuse = true;
        self.reason = reason.into();
        self
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_has_the_keys_the_sheet_reads() {
        let mut d = PlanDoc { summary: "1 change".into(), ..Default::default() };
        d.change("Packages", "linux", "6.1 → 6.2", None);
        let v: serde_json::Value = serde_json::from_str(&d.to_json()).unwrap();
        assert_eq!(v["changes"][0]["group"], "Packages");
        assert!(v["changes"][0].get("reversible").is_none());
        assert_eq!(v["would_refuse"], false);
    }
}
