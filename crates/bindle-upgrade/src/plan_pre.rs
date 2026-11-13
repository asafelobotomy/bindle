//! The stages before anything is upgraded: Bindle itself, then the checks
//! that hold the package stages back (room on disk, uQ's Hyprland pin,
//! battery power).

use super::{Action, Options, StagePlan, Todo, needs_restart, plural};
use crate::probe::Probe;
use crate::selfupdate::{Location, SelfState, newer};
use crate::stage::Stage;
use crate::system::efi;

const UPGRADES: [Stage; 3] = [Stage::Pacman, Stage::Aur, Stage::Releases];

pub(super) fn bindle(s: &SelfState) -> StagePlan {
    let st = Stage::Bindle;
    let tag = match &s.latest {
        _ if s.just_updated => return StagePlan::new(st, Todo::Nothing, format!("updated to {} just now", s.running)),
        None => return StagePlan::new(st, Todo::Nothing, "not checked"),
        Some(Err(e)) => return StagePlan::new(st, Todo::Report, format!("couldn't check for a new Bindle ({e})")),
        Some(Ok(tag)) if !newer(&s.running, tag) => {
            return StagePlan::new(st, Todo::Nothing, format!("{} is the latest", s.running));
        }
        Some(Ok(tag)) => tag.clone(),
    };
    let out = format!("Bindle {} is out (this is {})", tag.trim_start_matches('v'), s.running);
    match &s.location {
        Some(Location::Elsewhere(path)) => {
            StagePlan::new(st, Todo::Report, format!("{out}; this copy can't update itself")).details(vec![format!(
                "{path}: get the new one from https://github.com/{}/releases",
                crate::selfupdate::REPO
            )])
        }
        Some(loc) => {
            let action = Action::SelfUpdate { tag: tag.clone(), location: loc.clone() };
            StagePlan::new(st, Todo::Run(vec![action]), format!("{out}: update, then go on with the new one"))
                .ask(format!("Update Bindle to {}?", tag.trim_start_matches('v')), true)
        }
        None => StagePlan::new(st, Todo::Report, out),
    }
}

/// The checks, and the stages each finding holds back (with why).
pub(super) fn preflight(p: &Probe, o: &Options) -> (StagePlan, Vec<(Stage, String)>) {
    let s = Stage::Preflight;
    let updates: Vec<&crate::parse::Update> = p.pacman.iter().flatten().collect();
    let upgrading = !updates.is_empty() || p.aur.as_ref().is_ok_and(|l| !l.is_empty());
    let kernel = updates.iter().any(|u| needs_restart(&u.name) || u.name == "mkinitcpio");
    let mut lines = vec![];
    let mut holds: Vec<(Stage, String)> = vec![];
    let mut problems = 0;
    let mut hold = |stages: &[Stage], why: String, lines: &mut Vec<String>| {
        problems += 1;
        lines.push(why.clone());
        for st in stages {
            if !holds.iter().any(|(h, _)| h == st) {
                holds.push((*st, why.clone()));
            }
        }
    };
    // Room: `/` for the packages, /boot and the EFI partition for new images.
    let space = &o.prefs.space;
    let mut checks: Vec<(&crate::system::Mount, i64, &str)> = vec![];
    if let Some(m) = p.mounts.iter().find(|m| m.target == "/") {
        checks.push((m, space.root_mb, "packages"));
    }
    if kernel {
        let esp = efi(&p.mounts);
        if let Some(m) = p.mounts.iter().find(|m| m.target == "/boot" && esp.is_none_or(|e| e.target != "/boot")) {
            checks.push((m, space.boot_mb, "the new kernel images"));
        }
        if let Some(m) = esp {
            checks.push((m, space.boot_mb.max(space.efi_mb), "the new kernel images"));
        }
    }
    for (m, need, what) in checks {
        if upgrading && (m.avail_mb as i64) < need {
            let why = format!(
                "only {} MB free on {} ({need} MB needed for {what}): free some space first (bindo clean-pkg-cache)",
                m.avail_mb, m.target
            );
            hold(&UPGRADES, why, &mut lines);
        }
    }
    // uQ pins Hyprland to the series it is tested with.
    if let (Some(pin), Some(h)) = (&p.pin, updates.iter().find(|u| u.name == "hyprland")) {
        let range = format!(
            "{}{}",
            pin.at_least.as_ref().map_or(String::new(), |v| format!(">= {v} ")),
            pin.below.as_ref().map_or(String::new(), |v| format!("< {v}"))
        );
        if pin.allows(&h.new) {
            lines.push(format!("Hyprland {} is within uQ's tested range ({})", h.new, range.trim()));
        } else {
            let mut why = format!(
                "Hyprland {} is newer than uQ is tested with ({}, uq-hyprland-pin): pacman would refuse the whole upgrade",
                h.new,
                range.trim()
            );
            match p.aur.iter().flatten().find(|u| u.name == "uq-hyprland-pin") {
                Some(u) => {
                    why.push_str(&format!("; uQ's new pin {} is on the AUR: install it and the new Hyprland in one transaction, not one after the other", u.new))
                }
                None => why.push_str("; wait for uQ to update its pin, then update"),
            }
            hold(&[Stage::Pacman, Stage::Aur], why, &mut lines);
        }
    }
    // On battery, a big upgrade (or a low battery) waits for mains power.
    let mut ask = None;
    if let Some(pw) = p.power.filter(|pw| pw.on_battery && upgrading) {
        let low = pw.capacity.is_some_and(|c| i64::from(c) < o.prefs.battery_min_percent);
        let big = kernel || updates.len() as i64 >= o.prefs.battery_max_packages;
        if low || big {
            let charge = pw.capacity.map_or(String::new(), |c| format!(" ({c}%)"));
            let what = if kernel { "a kernel or driver update" } else { "a big upgrade" };
            let why = format!("on battery{charge} with {what}: plug in first");
            if o.on_battery {
                lines.push(format!("{why} (going ahead: --on-battery)"));
            } else if o.yes {
                hold(&UPGRADES, format!("{why}, or run with --on-battery"), &mut lines);
            } else {
                lines.push(why);
                ask = Some("Update on battery anyway?");
            }
        }
    }
    let pl = if lines.is_empty() {
        let what = if p.power.is_some_and(|pw| pw.on_battery) { "on battery" } else { "power is fine" };
        StagePlan::new(s, Todo::Nothing, format!("enough room; {what}"))
    } else {
        let summary = match (problems, ask) {
            (0, Some(_)) => "on battery: asked first".to_string(),
            (0, None) => lines[0].clone(),
            (n, _) => format!("{} holding the upgrade back", plural(n, "problem", "problems")),
        };
        let mut pl = StagePlan::new(s, Todo::Report, summary).details(lines);
        if let Some(q) = ask {
            pl = pl.ask(q, false);
            pl.stops = UPGRADES.to_vec();
        }
        pl
    };
    (pl, holds)
}
