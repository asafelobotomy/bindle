//! One snapper pair for a whole bindleUP run. `RunSnapshotPre` takes the
//! "pre" snapshot and pauses snap-pac (`SNAP_PAC_SKIP`, which every pacman
//! the helper runs inherits, the AUR helper's included); `RunSnapshotPost`,
//! or the end of the session, takes the matching "post" and resumes it. So a
//! run is one restore point, "bindleup", instead of a pair per pacman call.

use std::process::Command;

use anyhow::{Context, Result, bail};

use super::Helper;

/// The description both snapshots carry.
pub const DESCRIPTION: &str = "bindleup";

impl Helper {
    pub(super) fn run_snapshot_pre(&mut self) -> Result<u32> {
        if let Some(n) = self.run_pre {
            return Ok(n);
        }
        let argv = ["snapper", "-c", "root", "create", "-t", "pre", "-p", "-d", DESCRIPTION, "-c", "number"];
        let n = if self.opts.fake_commands {
            self.output(&format!("would run: {}", argv.join(" ")));
            0
        } else {
            self.log_line(&format!("$ {}", argv.join(" ")));
            let out = Command::new(argv[0]).args(&argv[1..]).output().context("start snapper")?;
            if !out.status.success() {
                bail!("snapper: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
            parse_number(&String::from_utf8_lossy(&out.stdout))?
        };
        // SAFETY: the helper is single-threaded; commands it starts from now on inherit this.
        unsafe { std::env::set_var("SNAP_PAC_SKIP", "y") };
        self.run_pre = Some(n);
        self.output(&format!("snapshot {n} (pre): before this run; snap-pac waits until the run ends"));
        Ok(n)
    }

    /// The "post" for the run's "pre", if there is one.
    pub(super) fn run_snapshot_post(&mut self) -> Result<()> {
        let Some(pre) = self.run_pre.take() else { return Ok(()) };
        // SAFETY: as above.
        unsafe { std::env::remove_var("SNAP_PAC_SKIP") };
        let pre = pre.to_string();
        let argv = [
            "snapper",
            "-c",
            "root",
            "create",
            "-t",
            "post",
            "--pre-number",
            &pre,
            "-p",
            "-d",
            DESCRIPTION,
            "-c",
            "number",
        ];
        if self.opts.fake_commands {
            self.output(&format!("would run: {}", argv.join(" ")));
            return Ok(());
        }
        self.log_line(&format!("$ {}", argv.join(" ")));
        let out = Command::new(argv[0]).args(&argv[1..]).output().context("start snapper")?;
        if !out.status.success() {
            bail!("snapper: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let n = parse_number(&String::from_utf8_lossy(&out.stdout))?;
        self.output(&format!("snapshot {n} (post): after this run, paired with {pre}"));
        Ok(())
    }
}

impl Helper {
    /// A run that ended without its "post" (the client died): close the pair.
    pub(super) fn close_run_snapshot(&mut self) {
        if let Err(e) = self.run_snapshot_post() {
            self.log_line(&format!("ERROR: {e:#}"));
        }
    }
}

/// `snapper create -p` prints the new snapshot's number.
pub(super) fn parse_number(out: &str) -> Result<u32> {
    out.trim().parse().with_context(|| format!("snapper printed `{}`, not a snapshot number", out.trim()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn snapper_prints_the_number() {
        assert_eq!(super::parse_number("530\n").unwrap(), 530);
        assert!(super::parse_number("").is_err());
    }
}
