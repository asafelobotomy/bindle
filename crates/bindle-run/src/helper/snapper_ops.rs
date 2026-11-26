//! Snapshot management for `bindle snapshot` (and uQ Settings' Restore
//! page): take one now, delete some, read and change the automatic-snapshot
//! settings. Every value is checked here, as root, before snapper sees it.

use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use bindle_core::recover::snapper_config;

use super::Helper;
use super::run_snapshot::parse_number;

/// The longest description kept.
const MAX_DESCRIPTION: usize = 100;

/// A description snapper can store: printable, one line, not too long.
pub fn clean_description(d: &str) -> String {
    let d: String = d.chars().filter(|c| !c.is_control()).take(MAX_DESCRIPTION).collect();
    let d = d.trim();
    if d.is_empty() { "bindle".to_string() } else { d.to_string() }
}

impl Helper {
    /// Run snapper with `args`; its standard output, or why it failed.
    fn snapper(&mut self, args: &[&str]) -> Result<String> {
        if self.opts.fake_commands {
            self.output(&format!("would run: snapper {}", args.join(" ")));
            return Ok(String::new());
        }
        self.log_line(&format!("$ snapper {}", args.join(" ")));
        let out = Command::new("snapper").args(args).output().context("start snapper")?;
        if !out.status.success() {
            bail!("snapper: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub(super) fn create_snapshot(&mut self, description: &str) -> Result<()> {
        let d = clean_description(description);
        let out = self.snapper(&["-c", "root", "create", "-c", "number", "-p", "-d", &d])?;
        if !self.opts.fake_commands {
            let n = parse_number(&out)?;
            self.output(&n.to_string());
        }
        Ok(())
    }

    pub(super) fn delete_snapshots(&mut self, numbers: &[u32]) -> Result<()> {
        if numbers.is_empty() {
            return Ok(());
        }
        if numbers.contains(&0) {
            bail!("snapshot 0 is the running system");
        }
        let shown: Vec<String> = numbers.iter().map(u32::to_string).collect();
        let mut args = vec!["-c", "root", "delete"];
        args.extend(shown.iter().map(String::as_str));
        self.snapper(&args)?;
        Ok(())
    }

    pub(super) fn snapper_settings(&mut self) -> Result<()> {
        let json = match self.snapper(&["--jsonout", "-c", "root", "get-config"]) {
            Ok(_) if self.opts.fake_commands => return Ok(()),
            Ok(out) => snapper_config::parse(&out).map_err(|e| anyhow!(e))?,
            // Older snapper: no --jsonout for get-config.
            Err(_) => snapper_config::parse_table(&self.snapper(&["-c", "root", "get-config"])?),
        };
        self.output(&serde_json::Value::Object(json).to_string());
        Ok(())
    }

    pub(super) fn set_snapper_setting(&mut self, key: &str, value: &str) -> Result<()> {
        if let Some(why) = snapper_config::check(key, value) {
            bail!(why);
        }
        self.log_line(&format!("== snapper setting {key}={value}"));
        self.snapper(&["-c", "root", "set-config", &format!("{key}={value}")])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::clean_description;

    #[test]
    fn descriptions_are_one_short_line() {
        assert_eq!(clean_description("before\nthe party"), "beforethe party");
        assert_eq!(clean_description("   "), "bindle");
        assert_eq!(clean_description(&"x".repeat(300)).len(), 100);
    }
}
