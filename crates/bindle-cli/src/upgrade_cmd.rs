//! `bindleup` (also `bindle upgrade`): update the whole system in one
//! guided run. The work is in the `bindle-upgrade` crate; this is the
//! terminal around it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::{upgrade_select, upgrade_stream};
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
    /// With --check: print the plan as JSON. With --dry-run: print what would
    /// change as one plan document (uQ Settings' confirmation sheet).
    #[arg(long)]
    json: bool,
    /// Progress as one JSON object per line (uQ Settings' Updates tab): the
    /// run, unattended like --yes with the AUR left to a terminal; with
    /// --check, the look at the system and its plan.
    #[arg(long, conflicts_with_all = ["gui", "json", "prefs", "notify", "dry_run"])]
    ndjson: bool,
    /// Root commands go to a helper that only records them under this directory.
    #[arg(long, hide = true)]
    sandbox: Option<PathBuf>,
}

fn stages(names: &[String]) -> Result<BTreeSet<Stage>> {
    names.iter().filter(|n| !n.trim().is_empty()).map(|n| n.parse().map_err(anyhow::Error::msg)).collect()
}

fn validate(a: &UpgradeArgs) -> Result<()> {
    if a.json && !(a.check || a.dry_run) {
        bail!("--json goes with --check (the plan) or --dry-run (what would change)");
    }
    Ok(())
}

pub fn main(mut a: UpgradeArgs) -> Result<()> {
    validate(&a)?;
    // Nobody is there to answer: take the default answers, as --yes does.
    a.yes |= a.ndjson;
    let prefs = prefs::load(&mut |w| eprintln!("bindleup: {w}"));
    if a.prefs {
        print!("{}", prefs.to_toml());
        return Ok(());
    }
    if a.gui {
        return crate::gui::upgrade(a.dry_run, a.sandbox.as_deref());
    }
    let rehearsal = a.dry_run || a.sandbox.is_some();
    if bindle_run::sys::is_root() && !a.check && !rehearsal {
        bail!("run bindleup as your user: it asks for the password when it needs to, and AUR builds refuse root");
    }
    let tty = bindle_run::tty::own_tty();
    let mut opts = Options {
        skip: stages(&a.skip)?,
        only: stages(&a.only)?,
        yes: a.yes,
        firmware: a.firmware,
        remove_orphans: a.remove_orphans,
        ignore_news: a.ignore_news,
        interactive: !a.yes && tty.is_some(),
        windowed: a.ndjson,
        on_battery: a.on_battery,
        prefs,
    };
    if let Some((what, todo)) = bindle_run::net::problem() {
        bail!("{what} {todo}");
    }
    let catalog = Catalog::arch()?;
    let quiet = a.json || a.notify || a.ndjson;
    if !quiet {
        println!("Checking what can be updated…");
    }
    let exe = std::env::current_exe()?;
    // A check only looks: fwupd's list is refreshed by a real run.
    let refresh = !a.check && !rehearsal;
    let probe = probe::gather(&catalog, &opts.prefs, &exe, refresh, &|s| opts.enabled(s), &mut |n| {
        if a.ndjson {
            upgrade_stream::emit(&serde_json::json!({ "event": "step", "text": n }));
        } else if !quiet {
            println!("  · {n}");
        }
    });
    let mut plans = plan(&probe, &opts);
    if a.ndjson && a.check {
        upgrade_stream::plan_event(&plans);
        return Ok(());
    }
    if a.json && a.dry_run {
        println!("{}", bindle_upgrade::preview::document(&plans).to_json());
        return Ok(());
    }
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
    // A run at a terminal asks once what to update; the rest is unattended.
    let mut relaunch_args = None;
    if opts.interactive && !a.ndjson && plans.iter().any(StagePlan::runs) {
        match choose(&mut opts, &probe, &plans) {
            Some(args) => {
                relaunch_args = Some(args);
                plans = plan(&probe, &opts);
            }
            None => {
                println!("Nothing was changed.");
                return Ok(());
            }
        }
    }
    let run_opts =
        RunOpts { yes: opts.yes, dry_run: a.dry_run, sandbox: a.sandbox.clone(), launch: Launch::default(), tty };
    let before = probe.after.clone();
    let mut after = |updated: &[String], boot: bindle_upgrade::run::BootView| {
        // A rehearsal changed nothing: the first look still holds.
        let now = if rehearsal { before.clone() } else { probe::after() };
        let now = boot.into_after(now);
        after_plans(&before, &now, updated, &opts, false)
    };
    let mut printer = Printer;
    let mut stream = upgrade_stream::Stream;
    let ui: &mut dyn Ui = if a.ndjson { &mut stream } else { &mut printer };
    let results = run(plans, &mut after, &run_opts, ui);
    if a.ndjson {
        results.iter().for_each(upgrade_stream::result_event);
    }
    if bindle_upgrade::run::relaunch(&results, rehearsal) {
        if a.ndjson {
            upgrade_stream::emit(&serde_json::json!({ "event": "relaunch" }));
        } else {
            println!("\nBindle updated itself; the new one goes on.");
        }
        return relaunch(&exe, relaunch_args);
    }
    if a.ndjson {
        let failed = report::any_failed(&results);
        upgrade_stream::emit(&serde_json::json!({ "event": "done", "failed": failed }));
        if failed {
            std::process::exit(1);
        }
        return Ok(());
    }
    println!("\nbindleup finished:");
    report::table(&results, rehearsal).iter().for_each(|l| println!("{l}"));
    // The Bindle that just replaced the old one: what it brings is next.
    let items = probe.after.setup.whats_new.len();
    if std::env::var_os(selfupdate::UPDATED_ENV).is_some() && !rehearsal {
        whats_new_next(run_opts.tty.is_some() && !a.yes, &exe, items);
    }
    if report::any_failed(&results) {
        std::process::exit(1);
    }
    Ok(())
}

/// The one question: what to update. Changes `opts` so every later question
/// is answered (`yes`), and returns the same answers as arguments for a
/// Bindle that replaces itself mid-run. None: the user quit.
fn choose(
    opts: &mut Options,
    probe: &bindle_upgrade::probe::Probe,
    plans: &[StagePlan],
) -> Option<Vec<std::ffi::OsString>> {
    let firmware = matches!(&probe.firmware, Some(Ok(l)) if !l.is_empty());
    let picked = upgrade_select::ask(
        upgrade_select::choices(plans, firmware),
        &mut std::io::stdin().lock(),
        &mut std::io::stdout(),
    )?;
    opts.skip.extend(picked.skip.iter().copied());
    opts.firmware = picked.firmware;
    opts.remove_orphans = picked.remove_orphans;
    // Questions whose answer is "no" unless the user says otherwise (an Arch
    // news post asking for a manual step, upgrading on battery) are asked
    // now, while they are here, not in the middle of the run.
    for p in plans.iter().filter(|p| !p.runs() && !p.stops.is_empty()) {
        let Some((question, false)) = &p.ask else { continue };
        let live = |s: &Stage| !picked.skip.contains(s) && plans.iter().any(|q| q.stage == *s && q.runs());
        if !p.stops.iter().any(live) {
            continue;
        }
        println!("\n{}: {}", p.stage.title(), p.summary);
        report::details(p).iter().for_each(|l| println!("{l}"));
        if crate::apply::confirm(question, false) {
            match p.stage {
                Stage::News => opts.ignore_news = true,
                Stage::Preflight => opts.on_battery = true,
                _ => {}
            }
        }
    }
    opts.yes = true;
    opts.interactive = false;
    println!("\nThe password is asked once; then it runs by itself. Problems are listed at the end.");
    let ids = |set: &BTreeSet<Stage>| set.iter().map(|s| s.id()).collect::<Vec<_>>().join(",");
    let mut args: Vec<std::ffi::OsString> = vec!["upgrade".into(), "--yes".into()];
    if !opts.skip.is_empty() {
        args.extend(["--skip".into(), ids(&opts.skip).into()]);
    }
    if !opts.only.is_empty() {
        args.extend(["--only".into(), ids(&opts.only).into()]);
    }
    for (on, flag) in [
        (opts.firmware, "--firmware"),
        (opts.remove_orphans, "--remove-orphans"),
        (opts.ignore_news, "--ignore-news"),
        (opts.on_battery, "--on-battery"),
    ] {
        if on {
            args.push(flag.into());
        }
    }
    Some(args)
}

/// After a self-update: the release notes once, then What's new. On a
/// terminal: print the notes and offer the review; otherwise a notification
/// (from its own process, so this run can end) with the summary.
fn whats_new_next(interactive: bool, exe: &std::path::Path, items: usize) {
    if interactive {
        let notes = crate::relnotes_cmd::show_pending();
        if items > 0
            && crate::apply::confirm(
                if notes { "Open What's new now?" } else { "\nReview what's new in this Bindle now?" },
                true,
            )
        {
            let _ = std::process::Command::new(exe).arg("whats-new").status();
        } else if items > 0 {
            println!("Any time: bindle whats-new");
        }
        return;
    }
    use std::os::unix::process::CommandExt;
    let _ = std::process::Command::new(exe).args(["whats-new", "--notify"]).process_group(0).spawn();
}

/// Run the rest with the new Bindle (same path, new file), the same
/// arguments, and `UPDATED_ENV` set so it doesn't update itself again.
fn relaunch(exe: &std::path::Path, chosen: Option<Vec<std::ffi::OsString>>) -> Result<()> {
    use std::os::unix::process::CommandExt;
    // After the one question, the new Bindle gets the answers as flags, so it doesn't ask again.
    let mut args: Vec<std::ffi::OsString> = chosen.unwrap_or_else(|| std::env::args_os().skip(1).collect());
    if args.first().is_none_or(|a| a != "upgrade") {
        args.insert(0, "upgrade".into());
    }
    // An AppImage was replaced at its own path, not at the mounted binary's.
    let target = std::env::var_os("APPIMAGE").map(PathBuf::from).unwrap_or_else(|| exe.to_path_buf());
    let err = std::process::Command::new(&target).args(args).env(selfupdate::UPDATED_ENV, "1").exec();
    Err(anyhow::anyhow!("couldn't start the new Bindle ({}): {err}", target.display()))
}

/// `--check --notify`: a notification when there is work; its Update
/// button opens the update window (or bindleup in a terminal).
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
        assert!(validate(&T::parse_from(["x", "--json"]).a).is_err(), "--json needs --check or --dry-run");
        assert!(validate(&T::parse_from(["x", "--json", "-n"]).a).is_ok());
        assert!(T::try_parse_from(["x", "--ndjson", "--json"]).is_err());
        assert!(T::parse_from(["x", "--ndjson", "--check"]).a.ndjson);
        assert!(T::try_parse_from(["x", "--gui", "--check"]).is_err());
        assert!(T::parse_from(["x", "--gui", "-n"]).a.gui);
    }
}
