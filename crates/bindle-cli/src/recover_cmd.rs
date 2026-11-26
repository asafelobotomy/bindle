//! `bindle rollback` (and, as they come, the other recovery tools): the
//! terminal side. The root helper does the work; this lists, asks and says
//! what happened.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{Result, bail};
use bindle_core::recover::snapper::{self, Snapshot};
use bindle_run::client::Launch;
use bindle_run::protocol::Request;
use bindle_run::session::Session;
use bindle_run::sys;

#[derive(clap::Subcommand, Debug)]
pub enum RecoverCmd {
    /// The Recovery window: Roll back, Rebase and Factory Restore, explained.
    Recovery {
        /// Print what this computer allows as JSON (rollback, restore,
        /// imageKept, snapper), open nothing.
        #[arg(long)]
        status: bool,
        /// Print what would run, run nothing.
        #[arg(long, short = 'n')]
        dry_run: bool,
        #[arg(long, hide = true)]
        sandbox: Option<PathBuf>,
    },
    /// Roll the system back to a snapper snapshot: it becomes the system at
    /// the next start; the current one is kept aside (Btrfs and snapper only).
    Rollback(RollbackArgs),
    /// Put the system back to how it was installed, Bindle's setup included:
    /// reinstall those packages, reset their changed config files (kept),
    /// apply Bindle's setup again. Your home and own packages stay.
    Rebase(rebase::RebaseArgs),
    /// Factory Restore: restart into the Arch installer to erase this
    /// computer and install it from new (nothing is erased until its last question).
    Restore(restore::RestoreArgs),
    /// The system's snapper snapshots: list, take one now, delete, and the
    /// automatic-snapshot settings.
    Snapshot {
        #[command(subcommand)]
        cmd: snapshot::SnapshotCmd,
    },
}

mod offline;
mod rebase;
mod restore;
mod snapshot;

#[derive(clap::Args, Debug)]
pub struct RollbackArgs {
    /// Snapshot to roll back to (see --list); asked when omitted.
    pub number: Option<u32>,
    /// List the snapshots, change nothing.
    #[arg(long)]
    pub list: bool,
    /// With --list: as JSON (the Recovery window reads this).
    #[arg(long, requires = "list")]
    pub json: bool,
    /// Don't ask for confirmation (nor to restart).
    #[arg(long, short)]
    pub yes: bool,
    /// Print what would run, run nothing.
    #[arg(long, short = 'n')]
    pub dry_run: bool,
    #[arg(long, hide = true)]
    pub sandbox: Option<PathBuf>,
    /// From the live system (the boot menu's "Roll back the system"): roll
    /// back this computer's system, which isn't running.
    #[arg(long, hide = true)]
    pub installed_system: bool,
}

pub fn main(cmd: RecoverCmd) -> Result<()> {
    match cmd {
        RecoverCmd::Recovery { status: true, .. } => {
            println!("{}", bindle_core::recover::status::status());
            Ok(())
        }
        RecoverCmd::Recovery { dry_run, sandbox, .. } => crate::gui::recovery(dry_run, sandbox.as_deref()),
        RecoverCmd::Rollback(a) => rollback(a),
        RecoverCmd::Rebase(a) => rebase::main(a),
        RecoverCmd::Restore(a) => restore::main(a),
        RecoverCmd::Snapshot { cmd } => snapshot::main(cmd),
    }
}

fn rollback(a: RollbackArgs) -> Result<()> {
    if a.installed_system {
        return offline::run(a.number, a.yes, a.dry_run);
    }
    let launch = Launch::default();
    let log = format!("/var/log/bindle/rollback-{}.log", sys::now_id());
    let mut session = Session::new(log, a.dry_run, a.sandbox.clone(), &launch);
    let print = &mut |l: &str| println!("{l}");
    let result = (|| -> Result<()> {
        let snaps = if a.dry_run && a.number.is_some() { vec![] } else { list(&mut session)? };
        if a.json {
            println!("{}", serde_json::to_string(&snaps)?);
            return Ok(());
        }
        if a.list || a.number.is_none() {
            show(&snaps);
        }
        if a.list {
            return Ok(());
        }
        let number = match a.number {
            Some(n) => n,
            None if snaps.is_empty() => bail!("there's no snapshot to roll back to"),
            None => pick(&snaps)?,
        };
        if !session.rehearsal() && !snaps.iter().any(|s| s.number == number) {
            bail!("there's no snapshot {number} (see `bindle rollback --list`)");
        }
        println!(
            "Rolling back makes snapshot {number} the system from the next start. Everything outside your home \
             folder goes back to how it was then; the system as it is now is kept aside, to go back to or remove later."
        );
        if !a.yes && !crate::apply::confirm(&format!("Roll back to snapshot {number}?"), false) {
            bail!("nothing was changed");
        }
        session.root(&Request::Rollback { number }, print)?;
        if !session.rehearsal() {
            if !a.yes && crate::apply::confirm("Restart now to start the rolled-back system?", true) {
                std::process::Command::new("systemctl").arg("reboot").status()?;
            } else {
                println!("Restart when you're ready: systemctl reboot");
            }
        }
        Ok(())
    })();
    session.quit();
    result
}

/// snapper's snapshots, through the root helper (/.snapshots is root's).
fn list(session: &mut Session) -> Result<Vec<Snapshot>> {
    let mut json = String::new();
    session.root(&Request::Snapshots, &mut |l| {
        json.push_str(l);
        json.push('\n');
    })?;
    if session.rehearsal() {
        return Ok(vec![]);
    }
    snapper::parse(&json).map_err(anyhow::Error::msg)
}

pub(super) fn show(snaps: &[Snapshot]) {
    if snaps.is_empty() {
        println!("No snapshots yet.");
        return;
    }
    println!("Snapshots, newest first:");
    for s in snaps {
        println!("  {}", s.label());
    }
}

pub(super) fn pick(snaps: &[Snapshot]) -> Result<u32> {
    print!("Roll back to which snapshot (number, Enter to cancel)? ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let line = line.trim();
    if line.is_empty() {
        bail!("nothing was changed");
    }
    match line.parse::<u32>() {
        Ok(n) if snaps.iter().any(|s| s.number == n) => Ok(n),
        _ => bail!("{line:?} isn't one of the snapshots listed"),
    }
}
