//! `bindle snapshot`: the system's snapper snapshots, by hand. List them,
//! take one now, delete some, and read or change the automatic-snapshot
//! settings. Reads try snapper as the user first (it answers when the config
//! allows this user), then ask for the password unless `--no-ask`; changes
//! go through the root helper. uQ Settings' Restore page runs these.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, anyhow, bail};
use bindle_core::recover::snapper;
use bindle_core::recover::snapper_config;
use bindle_run::client::Launch;
use bindle_run::protocol::Request;
use bindle_run::session::Session;
use bindle_run::sys;

/// The exit status of a read that would have to ask for the password.
pub const NEEDS_PASSWORD: i32 = 3;

#[derive(clap::Subcommand, Debug)]
pub enum SnapshotCmd {
    /// The snapshots, newest first.
    List(ReadArgs),
    /// Take a snapshot of the system now; prints its number.
    Create {
        /// What to call it (find it by this later).
        #[arg(default_value = "bindle")]
        description: String,
        #[command(flatten)]
        common: Common,
    },
    /// Delete snapshots by number.
    Delete {
        #[arg(required = true)]
        numbers: Vec<u32>,
        /// Don't ask for confirmation.
        #[arg(long, short)]
        yes: bool,
        #[command(flatten)]
        common: Common,
    },
    /// The automatic-snapshot settings (timeline and how many to keep).
    Settings(ReadArgs),
    /// Change one of those settings, e.g. `TIMELINE_LIMIT_DAILY 7`.
    Set {
        key: String,
        value: String,
        #[command(flatten)]
        common: Common,
    },
}

#[derive(clap::Args, Debug)]
pub struct ReadArgs {
    /// As JSON.
    #[arg(long)]
    pub json: bool,
    /// Never ask for the password: exit with status 3 when it would be needed.
    #[arg(long)]
    pub no_ask: bool,
    #[command(flatten)]
    pub common: Common,
}

#[derive(clap::Args, Debug)]
pub struct Common {
    /// Print what would run, run nothing.
    #[arg(long, short = 'n')]
    pub dry_run: bool,
    #[arg(long, hide = true)]
    pub sandbox: Option<PathBuf>,
}

pub fn main(cmd: SnapshotCmd) -> Result<()> {
    let c = match &cmd {
        SnapshotCmd::List(a) | SnapshotCmd::Settings(a) => &a.common,
        SnapshotCmd::Create { common, .. } | SnapshotCmd::Delete { common, .. } | SnapshotCmd::Set { common, .. } => {
            common
        }
    };
    let rehearsal = c.dry_run || c.sandbox.is_some();
    if !rehearsal && !std::path::Path::new(bindle_core::recover::status::SNAPPER_CONFIG).exists() {
        bail!("snapper isn't set up here, so there are no snapshots (Bindle's \"Btrfs snapshots\" module sets it up)");
    }
    match cmd {
        SnapshotCmd::List(a) => {
            let snaps = read(&a, &["--jsonout", "-c", "root", "list"], Request::Snapshots)?;
            let snaps = if snaps.is_empty() { vec![] } else { snapper::parse(&snaps).map_err(anyhow::Error::msg)? };
            if a.json {
                println!("{}", serde_json::to_string(&snaps)?);
            } else {
                super::show(&snaps);
            }
        }
        SnapshotCmd::Settings(a) => {
            let out = read(&a, &["--jsonout", "-c", "root", "get-config"], Request::SnapperSettings)?;
            let map = if out.is_empty() {
                Default::default()
            } else {
                snapper_config::parse(&out).unwrap_or_else(|_| snapper_config::parse_table(&out))
            };
            if a.json {
                println!("{}", serde_json::Value::Object(map));
            } else {
                map.iter().for_each(|(k, v)| println!("{k} = {}", v.as_str().unwrap_or_default()));
            }
        }
        SnapshotCmd::Create { description, common } => {
            let out = root(&common, &Request::CreateSnapshot { description })?;
            if let Some(n) = out.lines().last() {
                println!("{n}");
            }
        }
        SnapshotCmd::Delete { numbers, yes, common } => {
            let list = numbers.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
            if !yes {
                if !std::io::stdin().is_terminal() {
                    bail!("deleting snapshots needs --yes when there's no terminal to ask on");
                }
                if !crate::apply::confirm(&format!("Delete snapshot(s) {list}? This can't be undone."), false) {
                    bail!("nothing was changed");
                }
            }
            root(&common, &Request::DeleteSnapshots { numbers })?;
            println!("Deleted snapshot(s) {list}.");
        }
        SnapshotCmd::Set { key, value, common } => {
            if let Some(why) = snapper_config::check(&key, &value) {
                bail!(why);
            }
            root(&common, &Request::SetSnapperSetting { key: key.clone(), value: value.clone() })?;
            println!("{key} = {value}");
        }
    }
    Ok(())
}

/// A read: snapper as this user, else (unless `--no-ask`) through the root
/// helper. Empty in a rehearsal.
fn read(a: &ReadArgs, user_args: &[&str], req: Request) -> Result<String> {
    if a.common.dry_run || a.common.sandbox.is_some() {
        return root(&a.common, &req);
    }
    if let Ok(out) = Command::new("snapper").args(user_args).output()
        && out.status.success()
    {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    if a.no_ask {
        eprintln!("reading the snapshots needs the administrator password");
        std::process::exit(NEEDS_PASSWORD);
    }
    root(&a.common, &req)
}

/// One root helper request; what it printed, a line each.
fn root(c: &Common, req: &Request) -> Result<String> {
    let launch = Launch::default();
    let log = format!("/var/log/bindle/snapshot-{}.log", sys::now_id());
    let mut session = Session::new(log, c.dry_run, c.sandbox.clone(), &launch);
    let mut out = String::new();
    let rehearsal = session.rehearsal();
    let result = session.root(req, &mut |l| {
        if rehearsal {
            println!("{l}");
        } else {
            out.push_str(l);
            out.push('\n');
        }
    });
    session.quit();
    result.map_err(|e| anyhow!("{e:#}"))?;
    Ok(out)
}
