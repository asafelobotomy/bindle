//! Recovery requests: listing snapper's snapshots and rolling back to one.
//! The helper works out the layout and the script itself (from the mount
//! table): the client names only a snapshot number.

use std::fs;

use anyhow::{Context, Result, anyhow};
use bindle_core::recover::{rebase, rollback};
use serde::{Deserialize, Serialize};

use super::Helper;
use crate::protocol::{ConfigFile, Request};
use crate::sys::{check_system_path, host, now_id, valid_name};

/// Where a rebase keeps the config files it reset.
pub const REBASE_DIR: &str = "/var/lib/bindle/rebase";

/// The last rollback, for `bindo rollback-cleanup` and the Recovery window.
pub const ROLLBACK_RECORD: &str = "/var/lib/bindle/rollback.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RollbackRecord {
    /// The snapshot the system was rolled back to.
    pub number: u32,
    pub when: String,
    /// Where the root that was replaced is kept (from the top level).
    pub kept_as: String,
}

impl Helper {
    /// The recovery requests (Rollback, Rebase, Factory Restore).
    pub(super) fn recover(&mut self, req: Request) -> Result<()> {
        match req {
            Request::Snapshots => self.snapshots(),
            Request::CreateSnapshot { description } => self.create_snapshot(&description),
            Request::DeleteSnapshots { numbers } => self.delete_snapshots(&numbers),
            Request::SnapperSettings => self.snapper_settings(),
            Request::SetSnapperSetting { key, value } => self.set_snapper_setting(&key, &value),
            Request::Rollback { number } => {
                self.log_line(&format!("== roll back to snapshot {number}"));
                self.rollback(number)
            }
            Request::ResetConfig { id, files } => {
                self.log_line(&format!("== reset {} config file(s), backups in rebase/{id}", files.len()));
                self.reset_config(&id, &files)
            }
            Request::PrepareRestore { iso, sig } => {
                self.log_line(&format!("== Factory Restore from {iso}"));
                self.prepare_restore(&iso, &sig)
            }
            Request::KeepImage { iso, sig } => self.keep_image(&iso, &sig),
            Request::CancelRestore { keep_image } => self.cancel_restore(keep_image),
            _ => Err(anyhow!("not a recovery request")),
        }
    }

    /// snapper's list of the root config, as JSON, one `Output` per line.
    pub(super) fn snapshots(&mut self) -> Result<()> {
        if self.opts.fake_commands {
            self.output("would run: snapper --jsonout -c root list");
            return Ok(());
        }
        let out = std::process::Command::new("snapper")
            .args(["--jsonout", "-c", "root", "list"])
            .output()
            .context("start snapper")?;
        if !out.status.success() {
            return Err(anyhow!("snapper: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            self.output(line);
        }
        Ok(())
    }

    /// Make snapshot `number` the system from the next start.
    pub(super) fn rollback(&mut self, number: u32) -> Result<()> {
        if number == 0 {
            return Err(anyhow!("snapshot 0 is the running system"));
        }
        let info = fs::read_to_string("/proc/self/mountinfo").context("read the mount table")?;
        let layout = rollback::layout(&info).map_err(|e| anyhow!(e))?;
        let stamp = now_id();
        let script = rollback::script(&layout, number, &stamp);
        self.cmd_owned(&["sh".into(), "-ec".into(), script])?;
        let record = RollbackRecord { number, when: stamp.clone(), kept_as: rollback::aside(&layout, &stamp) };
        let path = host(self.root(), ROLLBACK_RECORD);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, serde_json::to_string_pretty(&record)?).with_context(|| format!("write {}", path.display()))
    }

    /// Rebase: changed config files back to their package's version. Each
    /// file is checked again here: under /etc, not one a rebase keeps.
    pub(super) fn reset_config(&mut self, id: &str, files: &[ConfigFile]) -> Result<()> {
        if !crate::snapshot::valid_id(id) {
            return Err(anyhow!("refusing rebase id {id:?}"));
        }
        let mut changed = Vec::with_capacity(files.len());
        for f in files {
            check_system_path(&f.path)?;
            if !f.path.starts_with("/etc/") || rebase::kept(&f.path) || !valid_name(&f.package) {
                return Err(anyhow!("refusing to reset {} ({})", f.path, f.package));
            }
            changed.push(rebase::Changed { package: f.package.clone(), path: f.path.clone() });
        }
        if changed.is_empty() {
            return Ok(());
        }
        let backup = host(self.root(), &format!("{REBASE_DIR}/{id}"));
        let script = rebase::reset_script(&changed, &backup.display().to_string());
        self.cmd_owned(&["sh".into(), "-ec".into(), script])
    }
}
