//! `bindleupgrade` (also `bindle upgrade`): update the whole system in one
//! guided run. The work is in the `bindle-upgrade` crate; this is the
//! terminal around it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Result, bail};
use bindle_core::Catalog;
use bindle_run::client::Launch;
use bindle_upgrade::plan::{Options, after_plans, plan};
use bindle_upgrade::run::{RunOpts, Ui, run};
use bindle_upgrade::{Stage, StagePlan, Todo, prefs, probe, report, selfupdate};
use clap::Args;

#[derive(Args, Debug, Default)]
pub struct UpgradeArgs {
    /// Only report what can be updated; change nothing, ask for no password.
    #[arg(long, short)]
    check: bool,
    /// Answer every question yes (unattended). Upgrades are still held when
    /// unread Arch news asks for a manual step.
    #[arg(long, short)]
    yes: bool,
    /// Leave out stages (comma-separated): bindle, news, preflight, snapshot,
    /// mirrors, pacman, aur, releases, flatpak, firmware, rebuild, uq,
    /// pacnew, orphans, services, setup, reboot.
    #[arg(long, value_delimiter = ',', value_name = "STAGE")]
    skip: Vec<String>,
    /// Run only these stages (comma-separated, as for --skip).
    #[arg(long, value_delimiter = ',', value_name = "STAGE")]
    only: Vec<String>,
    /// Install firmware updates (fwupd), not only report them.
    #[arg(long)]
    firmware: bool,
    /// Remove orphaned packages without asking.
    #[arg(long)]
    remove_orphans: bool,
    /// Upgrade on battery even when the upgrade is big (or the battery low).
    #[arg(long)]
    on_battery: bool,
    /// With --check: when there is work, show a desktop notification whose
    /// Update button opens the update window (the timer `bindo update-notify` runs this).
    #[arg(long, requires = "check")]
    notify: bool,
    /// Upgrade even when unread Arch news asks for a manual step.
    #[arg(long)]
    ignore_news: bool,
    /// Print each command instead of running it.
    #[arg(long, short = 'n')]
    dry_run: bool,
    /// Open the update window (Bindle's desktop look) instead of the terminal.
    #[arg(long, conflicts_with_all = ["check", "yes", "json"])]
    gui: bool,
    /// Print the preferences in effect, as a file to save (see `bindo upgrade-prefs`).
    #[arg(long, conflicts_with_all = ["check", "dry_run"])]
    prefs: bool,
    /// With --check: print the plan as JSON.
    #[arg(long, requires = "check")]
    json: bool,
    /// Root commands go to a helper that only records them under this directory.
    #[arg(long, hide = true)]
    sandbox: Option<PathBuf>,
}

fn stages(names: &[String]) -> Result<BTreeSet<Stage>> {
    names.iter().filter(|n| !n.trim().is_empty()).map(|n| n.parse().map_err(anyhow::Error::msg)).collect()
}

pub fn main(a: UpgradeArgs) -> Result<()> {
    let prefs = prefs::load(&mut |w| eprintln!("bindleupgrade: {w}"));
    if a.prefs {
        print!("{}", prefs.to_toml());
        return Ok(());
    }
    if a.gui {
        return crate::gui::upgrade(a.dry_run, a.sandbox.as_deref());
    }
    let rehearsal = a.dry_run || a.sandbox.is_some();
    if bindle_run::sys::is_root() && !a.check && !rehearsal {
        bail!("run bindleupgrade as your user: it asks for the password when it needs to, and AUR builds refuse root");
    }
    let tty = bindle_run::tty::own_tty();
    let opts = Options {
        skip: stages(&a.skip)?,
        only: stages(&a.only)?,
        yes: a.yes,
        firmware: a.firmware,
        remove_orphans: a.remove_orphans,
        ignore_news: a.ignore_news,
        interactive: !a.yes && tty.is_some(),
        windowed: false,
        on_battery: a.on_battery,
        prefs,
    };
    if let Some((what, todo)) = bindle_run::net::problem() {
        bail!("{what} {todo}");
    }
    let catalog = Catalog::arch()?;
    let quiet = a.json || a.notify;
    if !quiet {
        println!("Checking what can be updated…");
    }
    let exe = std::env::current_exe()?;
    // A check only looks: fwupd's list is refreshed by a real run.
    let refresh = !a.check && !rehearsal;
    let probe = probe::gather(&catalog, &opts.prefs, &exe, refresh, &|s| opts.enabled(s), &mut |n| {
        if !quiet {
            println!("  · {n}");
        }
    });
    let plans = plan(&probe, &opts);
    if a.check {
        if a.notify {
            return notify(&plans);
        }
        if a.json {
            println!("{}", serde_json::to_string_pretty(&plans)?);
        } else {
            println!();
            report::check(&plans).iter().for_each(|l| println!("{l}"));
        }
        return Ok(());
    }
    let run_opts =
        RunOpts { yes: a.yes, dry_run: a.dry_run, sandbox: a.sandbox.clone(), launch: Launch::default(), tty };
    let before = probe.after.clone();
    let mut after = |updated: &[String], boot: Option<Vec<String>>| {
        // A rehearsal changed nothing: the first look still holds.
        let mut now = if rehearsal { before.clone() } else { probe::after() };
        now.boot = boot.or(now.boot);
        after_plans(&before, &now, updated, &opts, false)
    };
    let results = run(plans, &mut after, &run_opts, &mut Printer);
    if bindle_upgrade::run::relaunch(&results, rehearsal) {
        println!("\nBindle updated itself; the new one goes on.");
        return relaunch(&exe);
    }
    println!("\nbindleupgrade finished:");
    report::table(&results, rehearsal).iter().for_each(|l| println!("{l}"));
    // The Bindle that just replaced the old one: what it brings is next.
    if std::env::var_os(selfupdate::UPDATED_ENV).is_some() && !rehearsal && !probe.after.setup.whats_new.is_empty() {
        whats_new_next(run_opts.tty.is_some() && !a.yes, &exe);
    }
    if report::any_failed(&results) {
        std::process::exit(1);
    }
    Ok(())
}

/// After a self-update: on a terminal, offer What's new now; otherwise a
/// notification (from its own process, so this run can end).
fn whats_new_next(interactive: bool, exe: &std::path::Path) {
    if interactive {
        if crate::apply::confirm("\nReview what's new in this Bindle now?", true) {
            let _ = std::process::Command::new(exe).arg("whats-new").status();
        } else {
            println!("Any time: bindle whats-new");
        }
        return;
    }
    use std::os::unix::process::CommandExt;
    let _ = std::process::Command::new(exe).args(["whats-new", "--notify"]).process_group(0).spawn();
}

/// Run the rest with the new Bindle (same path, new file), the same
/// arguments, and `UPDATED_ENV` set so it doesn't update itself again.
fn relaunch(exe: &std::path::Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_none_or(|a| a != "upgrade") {
        args.insert(0, "upgrade".into());
    }
    // An AppImage was replaced at its own path, not at the mounted binary's.
    let target = std::env::var_os("APPIMAGE").map(PathBuf::from).unwrap_or_else(|| exe.to_path_buf());
    let err = std::process::Command::new(&target).args(args).env(selfupdate::UPDATED_ENV, "1").exec();
    Err(anyhow::anyhow!("couldn't start the new Bindle ({}): {err}", target.display()))
}

/// `--check --notify`: a notification when there is work; its Update
/// button opens the update window (or bindleupgrade in a terminal).
fn notify(plans: &[StagePlan]) -> Result<()> {
    let work: Vec<String> =
        plans.iter().filter(|p| p.runs()).map(|p| format!("{}: {}", p.stage.title(), p.summary)).collect();
    if work.is_empty() {
        return Ok(());
    }
    let summary =
        if work.len() == 1 { "An update is ready".to_string() } else { format!("{} updates are ready", work.len()) };
    let body = work.iter().take(4).cloned().collect::<Vec<_>>().join("\n");
    if crate::notify::show(&summary, &body, &[("update", "Update"), ("later", "Later")])?.as_deref() == Some("update") {
        let exe = std::env::current_exe()?;
        let args: &[&str] =
            if crate::gui::available().is_some() { &["upgrade", "--gui"] } else { &["in-terminal", "upgrade"] };
        std::process::Command::new(exe).args(args).spawn()?;
    }
    Ok(())
}

struct Printer;

impl Ui for Printer {
    fn stage(&mut self, index: usize, total: usize, plan: &StagePlan) {
        println!("\n==> [{}/{total}] {}: {}", index + 1, plan.stage.title(), plan.summary);
        if !matches!(plan.todo, Todo::Skip(_) | Todo::Nothing) {
            report::details(plan).iter().for_each(|l| println!("{l}"));
        }
        if let Todo::Hold(why) = &plan.todo {
            println!("    held: {why}");
        }
    }

    fn line(&mut self, line: &str) {
        println!("    │ {line}");
    }

    fn ask(&mut self, question: &str, default: bool) -> bool {
        crate::apply::confirm(question, default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct T {
        #[command(flatten)]
        a: UpgradeArgs,
    }

    #[test]
    fn stage_lists_parse() {
        let t = T::parse_from(["x", "--skip", "aur,flatpak", "--skip", "fwupd", "--check"]);
        assert_eq!(stages(&t.a.skip).unwrap(), [Stage::Aur, Stage::Flatpak, Stage::Firmware].into());
        assert!(stages(&["bogus".into()]).is_err());
        assert!(T::try_parse_from(["x", "--json"]).is_err(), "--json needs --check");
        assert!(T::try_parse_from(["x", "--gui", "--check"]).is_err());
        assert!(T::parse_from(["x", "--gui", "-n"]).a.gui);
    }
}
