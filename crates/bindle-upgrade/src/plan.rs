//! Decide what each stage does, from a [`Probe`] and the options. Pure: the
//! same data always gives the same plan, and nothing here touches the system.

use std::collections::BTreeSet;

use bindle_core::catalog::ReleaseAsset;
use serde::Serialize;

use crate::news;
use crate::parse::ymd;
use crate::prefs::Prefs;
use crate::probe::{After, Probe};
use crate::selfupdate::Location;
use crate::stage::Stage;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub skip: BTreeSet<Stage>,
    /// When not empty, only these stages.
    pub only: BTreeSet<Stage>,
    /// Unattended: questions take their "yes" answer, prompts are skipped.
    pub yes: bool,
    /// Install firmware updates, not only report them.
    pub firmware: bool,
    pub remove_orphans: bool,
    /// Upgrade even when unread news asks for manual intervention.
    pub ignore_news: bool,
    /// Someone is at the terminal: pacman runs on it and asks its own
    /// questions (conflicts, replacements) instead of taking its defaults.
    pub interactive: bool,
    /// Running from the desktop window: nothing can be typed into a
    /// terminal, so the AUR (whose helper reviews and asks for its password
    /// on one) is reported, to be updated in a terminal.
    pub windowed: bool,
    /// Upgrade on battery power even when the upgrade is big.
    pub on_battery: bool,
    /// Mirror and news ages, space, power, and where pinned apps update from.
    pub prefs: Prefs,
}

impl Options {
    pub fn enabled(&self, s: Stage) -> bool {
        !self.skip.contains(&s) && (self.only.is_empty() || self.only.contains(&s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Action {
    /// Through Bindle's root helper.
    Root(Vec<String>),
    /// Through the root helper, on the user's terminal, so it can ask.
    RootTerminal(Vec<String>),
    /// As the user, with the terminal (AUR helpers and Flatpak ask for
    /// their own permissions).
    User(Vec<String>),
    /// Download, check against the pin, install (packages through the helper).
    Releases(Vec<ReleaseAsset>),
    /// Asked on its own (default answer), then run: a clean-up after an update.
    Optional { question: String, default: bool, action: Box<Action> },
    /// Download this Bindle release, verify it, replace this Bindle; the
    /// run then continues with the new one.
    SelfUpdate { tag: String, location: Location },
    /// `uqctl setup --apps --json` as the user, reporting each file.
    UqSetup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Todo {
    Run(Vec<Action>),
    /// Something to tell; nothing to run.
    Report,
    /// Up to date.
    Nothing,
    /// Doesn't apply here, or turned off.
    Skip(String),
    /// Would run, but is held back (why).
    Hold(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct StagePlan {
    pub stage: Stage,
    pub todo: Todo,
    pub summary: String,
    pub details: Vec<String>,
    /// Asked before the stage runs: (question, default). `--yes` answers yes.
    pub ask: Option<(String, bool)>,
    /// Held as well when the question is answered no.
    pub stops: Vec<Stage>,
    /// Packages this stage upgrades (to tell whether a reboot is due).
    pub packages: Vec<String>,
}

impl StagePlan {
    fn new(stage: Stage, todo: Todo, summary: impl Into<String>) -> Self {
        StagePlan { stage, todo, summary: summary.into(), details: vec![], ask: None, stops: vec![], packages: vec![] }
    }

    fn details(mut self, d: Vec<String>) -> Self {
        self.details = d;
        self
    }

    fn ask(mut self, q: impl Into<String>, default: bool) -> Self {
        self.ask = Some((q.into(), default));
        self
    }

    pub fn runs(&self) -> bool {
        matches!(self.todo, Todo::Run(_))
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn root(argv: &[&str]) -> Action {
    Action::Root(argv.iter().map(|s| s.to_string()).collect())
}

fn user(argv: &[&str]) -> Action {
    Action::User(argv.iter().map(|s| s.to_string()).collect())
}

/// Every stage, in order. The closing ones come from the system as it is
/// now; [`after_plans`] makes them again once the upgrades ran.
pub fn plan(p: &Probe, o: &Options) -> Vec<StagePlan> {
    let (pre, holds) = preflight(p, o);
    let mut plans = vec![bindle(&p.bindle), news(p, o), pre];
    plans.extend([StagePlan::new(Stage::Snapshot, Todo::Nothing, ""), mirrors(p, o)]);
    plans.push(pacman(p, o));
    plans.push(aur(p, o));
    plans.push(releases(&p.releases, &o.prefs));
    plans.push(flatpak(p));
    plans.push(firmware(p, o));
    let pending: Vec<String> =
        p.pacman.iter().flatten().chain(p.aur.iter().flatten()).map(|u| u.name.clone()).collect();
    plans.extend(after_plans(&p.after, &p.after, &pending, o, true));
    finish(plans, p, o, &holds)
}

/// The closing stages, from a look after the upgrades (`now`) and the one
/// before (`before`). `updated`: packages upgraded (or, for `--check`, about
/// to be: `pending`).
pub fn after_plans(before: &After, now: &After, updated: &[String], o: &Options, pending: bool) -> Vec<StagePlan> {
    let plans = vec![
        rebuild(now, updated, o, pending),
        uq(before, now, updated, pending),
        pacfiles(&now.pacfiles),
        orphans(&now.orphans, o),
        services(before, now, pending),
        setup(&now.setup),
        reboot(now, updated, pending),
    ];
    plans.into_iter().map(|pl| turned_off(pl, o)).collect()
}

/// Apply what one stage means for another, then `--skip` / `--only`.
fn finish(mut plans: Vec<StagePlan>, p: &Probe, o: &Options, holds: &[(Stage, String)]) -> Vec<StagePlan> {
    plans = plans.into_iter().map(|pl| turned_off(pl, o)).collect();
    // Not enough room, uQ's Hyprland pin, battery under --yes.
    for pl in &mut plans {
        if let Some((_, why)) = holds.iter().find(|(s, _)| *s == pl.stage)
            && pl.runs()
        {
            pl.todo = Todo::Hold(why.clone());
        }
    }
    // Unattended, with news asking for a manual step: hold the upgrades.
    let held = o.yes && plans.iter().any(|pl| pl.stage == Stage::News && !pl.stops.is_empty());
    for pl in &mut plans {
        if held && pl.runs() && matches!(pl.stage, Stage::Pacman | Stage::Aur | Stage::Releases) {
            pl.todo = Todo::Hold("the Arch news asks for a manual step: read it, then run with --ignore-news".into());
        }
    }
    let upgrading = plans.iter().any(|pl| matches!(pl.stage, Stage::Pacman | Stage::Aur) && pl.runs());
    let pacman_runs = plans.iter().any(|pl| pl.stage == Stage::Pacman && pl.runs());
    for pl in &mut plans {
        match pl.stage {
            Stage::Snapshot => *pl = turned_off(snapshot(p, upgrading), o),
            Stage::Mirrors if !pacman_runs && pl.runs() => {
                *pl = StagePlan::new(Stage::Mirrors, Todo::Nothing, "no package upgrades");
            }
            _ => {}
        }
    }
    plans
}

fn turned_off(pl: StagePlan, o: &Options) -> StagePlan {
    if o.enabled(pl.stage) {
        return pl;
    }
    let why = if o.skip.contains(&pl.stage) { "turned off (--skip)" } else { "turned off (not in --only)" };
    StagePlan::new(pl.stage, Todo::Skip(why.into()), why)
}

fn news(p: &Probe, o: &Options) -> StagePlan {
    let items = match &p.news {
        Ok(items) => news::since(items, p.last_upgrade, p.today, o.prefs.news_days),
        Err(e) => {
            return StagePlan::new(Stage::News, Todo::Report, format!("couldn't read the news ({e})"))
                .details(vec!["Check https://archlinux.org/news/ before upgrading.".into()]);
        }
    };
    if items.is_empty() {
        let since = p.last_upgrade.map_or(format!("in the last {} days", o.prefs.news_days), |d| {
            format!("since your last upgrade ({})", ymd(d))
        });
        return StagePlan::new(Stage::News, Todo::Nothing, format!("nothing new {since}"));
    }
    let details = items
        .iter()
        .flat_map(|i| {
            let mark = if i.manual() { "  ⚠ manual step" } else { "" };
            [format!("{}  {}{mark}", ymd(i.day), i.title), format!("    {}", i.link)]
        })
        .collect();
    let manual = items.iter().filter(|i| i.manual()).count();
    let mut pl =
        StagePlan::new(Stage::News, Todo::Report, plural(items.len(), "new post", "new posts")).details(details);
    if manual > 0 && !o.ignore_news {
        pl.summary = format!("{}, {} a manual step", pl.summary, plural(manual, "asks for", "ask for"));
        pl = pl.ask("Have you read it and done what it says? Go on with the upgrade?", false);
        pl.stops = vec![Stage::Snapshot, Stage::Pacman, Stage::Aur, Stage::Releases];
    }
    pl
}

fn snapshot(p: &Probe, upgrading: bool) -> StagePlan {
    let s = Stage::Snapshot;
    if !p.snapper_root {
        StagePlan::new(s, Todo::Skip("snapper isn't set up for /".into()), "snapper isn't set up for /")
    } else if p.snap_pac {
        StagePlan::new(s, Todo::Skip("snap-pac does it".into()), "snap-pac snapshots every pacman run")
    } else if !upgrading {
        StagePlan::new(s, Todo::Nothing, "no package upgrades")
    } else {
        let argv =
            ["snapper", "-c", "root", "create", "--description", "bindleupgrade", "--cleanup-algorithm", "number"];
        StagePlan::new(s, Todo::Run(vec![root(&argv)]), "snapshot of / before upgrading")
    }
}

fn mirrors(p: &Probe, o: &Options) -> StagePlan {
    let s = Stage::Mirrors;
    match p.mirrorlist_age_days {
        _ if !p.reflector => {
            StagePlan::new(s, Todo::Skip("reflector isn't installed".into()), "reflector isn't installed")
        }
        Some(age) if age < o.prefs.mirror_max_age_days => {
            StagePlan::new(s, Todo::Nothing, format!("mirror list is {}", plural(age as usize, "day old", "days old")))
        }
        age => {
            let why = age.map_or("no mirror list".to_string(), |a| format!("mirror list is {a} days old"));
            StagePlan::new(
                s,
                Todo::Run(vec![root(&["systemctl", "start", "reflector.service"])]),
                format!("{why}; refreshing"),
            )
        }
    }
}

#[path = "plan_pkgs.rs"]
mod pkgs;
use pkgs::{aur, firmware, flatpak, pacman, releases};
#[path = "plan_pre.rs"]
mod pre;
use pre::{bindle, preflight};
#[path = "plan_after.rs"]
mod after;
use after::{orphans, pacfiles, rebuild, uq};
#[path = "plan_close.rs"]
mod close;
pub use close::needs_restart;
use close::{reboot, services, setup};

#[cfg(test)]
#[path = "plan_safety_tests.rs"]
mod safety_tests;
#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
