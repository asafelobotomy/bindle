//! Undo of an apply: the files its snapshot recorded put back, its units disabled.

use anyhow::{Result, bail};
use bindle_core::fstab;

use super::{Helper, check_names};
use crate::snapshot;

impl Helper {
    /// Put back the files a snapshot recorded and disable its units.
    pub(super) fn restore_snapshot(&mut self, id: &str, force: bool) -> Result<()> {
        let m = snapshot::load(self.root(), id)?;
        let report = snapshot::restore_files(self.root(), &m, force);
        for line in report.done.iter().chain(&report.kept) {
            self.output(line);
        }
        if !report.kept.is_empty() {
            self.output(&format!("to put the originals back anyway: bindle undo --force {id}"));
        }
        let mut errors = report.errors;
        // systemd made mount units from the fstab that was just put back.
        if m.files.iter().any(|f| f.path == fstab::PATH)
            && let Err(e) = self.cmd(&["systemctl", "daemon-reload"])
        {
            errors.push(format!("reload systemd: {e:#}"));
        }
        if !m.units.is_empty() {
            let units = m.units.clone();
            let disabled = check_names(&units).and_then(|()| {
                let mut argv: Vec<String> = ["systemctl", "disable", "--now", "--"].map(String::from).into();
                argv.extend(units);
                self.cmd_owned(&argv)
            });
            if let Err(e) = disabled {
                errors.push(format!("disable units: {e:#}"));
            }
        }
        if !errors.is_empty() {
            for e in &errors {
                self.output(&format!("not restored: {e}"));
            }
            bail!("snapshot {id}: {} item(s) could not be restored (listed above)", errors.len());
        }
        snapshot::mark_undone(self.root(), id)?;
        crate::record::on_undone(self.root(), id)
    }
}
