//! `bindle updates`: what the last check found and the bar-icon preference,
//! read from files only (no network, no password), for uQ's bar icon.

use anyhow::Result;
use bindle_upgrade::{prefs, status};
use clap::Args;
use serde_json::json;

#[derive(Args, Debug)]
pub struct UpdatesArgs {
    #[arg(long)]
    pub json: bool,
}

/// After this long, the last check is out of date: the bar icon checks again.
pub const STALE_SECS: u64 = 6 * 3600;

pub fn main(as_json: bool) -> Result<()> {
    let p = prefs::load(&mut |_| {});
    let saved = status::load();
    let v = view(p.bar_icon.word(), saved.as_ref(), status::now());
    if as_json {
        println!("{v}");
        return Ok(());
    }
    match &saved {
        None => println!("Not checked yet: bindleup --check"),
        Some(s) if s.items.is_empty() => println!("Up to date (checked {} ago)", ago(v["age"].as_u64().unwrap_or(0))),
        Some(s) => {
            println!("Ready to update (checked {} ago):", ago(v["age"].as_u64().unwrap_or(0)));
            for i in &s.items {
                let held = i.held.as_deref().map(|w| format!(" (held: {w})")).unwrap_or_default();
                println!("  {}: {}{held}", i.title, i.summary);
            }
        }
    }
    Ok(())
}

fn view(bar_icon: &str, saved: Option<&status::Saved>, now: u64) -> serde_json::Value {
    match saved {
        None => json!({ "bar_icon": bar_icon, "checked": null, "age": null, "stale": true, "count": 0, "items": [] }),
        Some(s) => {
            let age = now.saturating_sub(s.checked);
            json!({
                "bar_icon": bar_icon,
                "checked": s.checked,
                "age": age,
                "stale": age >= STALE_SECS || s.checked > now,
                "count": s.items.len(),
                "items": s.items,
            })
        }
    }
}

fn ago(secs: u64) -> String {
    match secs {
        s if s < 120 => "a minute".into(),
        s if s < 7200 => format!("{} minutes", s / 60),
        s if s < 172_800 => format!("{} hours", s / 3600),
        s => format!("{} days", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_checked_is_stale_and_empty() {
        let v = view("updates", None, 100);
        assert_eq!((v["stale"].as_bool(), v["count"].as_u64()), (Some(true), Some(0)));
        assert!(v["checked"].is_null());
    }

    #[test]
    fn a_check_goes_stale_after_six_hours() {
        let item = status::Item { stage: "pacman".into(), title: "Packages".into(), summary: "3".into(), held: None };
        let s = status::Saved { checked: 1000, items: vec![item] };
        let fresh = view("always", Some(&s), 1000 + 60);
        assert_eq!((fresh["stale"].as_bool(), fresh["count"].as_u64()), (Some(false), Some(1)));
        assert_eq!(fresh["bar_icon"], "always");
        assert_eq!(fresh["items"][0]["title"], "Packages");
        assert_eq!(view("always", Some(&s), 1000 + STALE_SECS)["stale"], true);
        assert_eq!(view("always", Some(&s), 10)["stale"], true, "a clock set back");
    }
}
