//! `bindle config show|set|reset`: bindleup's preferences (`upgrade.toml`)
//! and its daily update check, as typed keys. uQ Settings' page reads
//! `show --json` and writes through `set`.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};
use bindle_upgrade::prefs::{self, SYSTEM};
use bindle_upgrade::prefs_cfg as cfg;
use clap::Subcommand;
use serde_json::json;

/// The daily check's user timer, installed by `bindo update-notify on`.
const TIMER: &str = "bindle-update-check.timer";
const NOTIFY_KEY: &str = "update_notify";

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Every preference with its value, default and source.
    Show {
        #[arg(long)]
        json: bool,
    },
    /// Change one preference in your upgrade.toml. Keys: see `config show`,
    /// `releases.apps.<app>` (source|bindle) and `update_notify` (on|off).
    Set {
        key: String,
        value: String,
        /// Check the value and say what would be written; change nothing.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Go back to the default (or the system's value) for a key.
    Reset {
        key: String,
        #[arg(long)]
        json: bool,
    },
}

fn timer_path() -> Option<PathBuf> {
    prefs::user_path().and_then(|p| Some(p.parent()?.parent()?.join("systemd/user").join(TIMER)))
}

fn notify_on() -> bool {
    timer_path().is_some_and(|p| p.is_file())
}

pub fn main(cmd: ConfigCmd) -> Result<()> {
    let Some(user_path) = prefs::user_path() else { bail!("no home folder: can't find your upgrade.toml") };
    let user = std::fs::read_to_string(&user_path).ok();
    match cmd {
        ConfigCmd::Show { json } => show(user.as_deref(), &user_path, json),
        ConfigCmd::Set { key, value, dry_run, json } => {
            set(&user_path, user.as_deref(), &key, Some(&value), dry_run, json)
        }
        ConfigCmd::Reset { key, json } => set(&user_path, user.as_deref(), &key, None, false, json),
    }
}

fn show(user: Option<&str>, path: &std::path::Path, as_json: bool) -> Result<()> {
    let system = std::fs::read_to_string(SYSTEM).ok();
    let s = cfg::show(system.as_deref(), user, &path.display().to_string());
    if as_json {
        let mut v = serde_json::to_value(&s)?;
        v["update_notify"] = json!(notify_on());
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    for e in &s.keys {
        println!("{:<24} {} {}  ({})", e.key, e.value, e.unit, e.source);
    }
    for a in &s.apps {
        println!("releases.apps.{:<10} {}  ({})", a.app, a.value, a.source);
    }
    println!("{NOTIFY_KEY:<24} {}", if notify_on() { "on" } else { "off" });
    Ok(())
}

fn set(
    path: &std::path::Path,
    user: Option<&str>,
    key: &str,
    value: Option<&str>,
    dry: bool,
    as_json: bool,
) -> Result<()> {
    let say = |changed: bool, shown: &str| -> Result<()> {
        if as_json {
            println!("{}", json!({ "ok": true, "key": key, "value": shown, "changed": changed, "dry_run": dry }));
        } else if dry {
            println!("Would set {key} = {shown}");
        } else {
            println!("{key} = {shown}");
        }
        Ok(())
    };
    if key == NOTIFY_KEY {
        let want = match value {
            Some("on") => "on",
            Some("off") | None => "off",
            _ => bail!("{NOTIFY_KEY} is on or off"),
        };
        let changed = (want == "on") != notify_on();
        if changed && !dry {
            let out = Command::new(std::env::current_exe()?).args(["bindo", "update-notify", want]).output()?;
            if !out.status.success() {
                bail!(
                    "{}",
                    String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("bindo update-notify failed")
                );
            }
        }
        return say(changed, want);
    }
    let text = cfg::edited(user, key, value).map_err(anyhow::Error::msg)?;
    let changed = user != Some(text.as_str());
    if changed && !dry {
        cfg::save(path, &text)?;
    }
    say(changed, value.unwrap_or("default"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct T {
        #[command(subcommand)]
        c: ConfigCmd,
    }

    #[test]
    fn the_subcommands_parse() {
        assert!(matches!(T::parse_from(["x", "show", "--json"]).c, ConfigCmd::Show { json: true }));
        assert!(matches!(
            T::parse_from(["x", "set", "news_days", "7", "--dry-run"]).c,
            ConfigCmd::Set { dry_run: true, .. }
        ));
        assert!(T::try_parse_from(["x", "set", "news_days"]).is_err());
    }

    #[test]
    fn set_writes_only_a_valid_value() {
        let dir = std::env::temp_dir().join(format!("bindle-config-{}", std::process::id()));
        let file = dir.join("bindle/upgrade.toml");
        set(&file, None, "news_days", Some("12"), false, false).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("news_days = 12"));
        let before = std::fs::read_to_string(&file).unwrap();
        assert!(set(&file, Some(&before), "news_days", Some("-3"), false, false).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        set(&file, Some(&before), "news_days", None, false, false).unwrap();
        assert!(!std::fs::read_to_string(&file).unwrap().contains("news_days"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
