//! The package stages: pacman, the AUR, Bindle's pinned releases, Flatpak
//! and firmware.

use super::{Action, Options, StagePlan, Todo, plural, root, user};
use crate::leftovers::Leftover;
use crate::parse::Update;
use crate::prefs::{Follow, Prefs};
use crate::probe::Probe;
use crate::releases::{Newer, Source};
use crate::stage::Stage;

pub(super) fn pacman(p: &Probe, o: &Options) -> StagePlan {
    let s = Stage::Pacman;
    let list = match &p.pacman {
        Ok(l) if l.is_empty() => return StagePlan::new(s, Todo::Nothing, "up to date"),
        Ok(l) => l,
        Err(e) if e == "not checked" => return StagePlan::new(s, Todo::Nothing, "not checked"),
        Err(e) => {
            let pl = StagePlan::new(
                s,
                Todo::Run(vec![upgrade(o)]),
                format!("couldn't list updates ({e}); pacman shows them"),
            );
            return if o.interactive { pl } else { pl.ask("Upgrade the system?", true) };
        }
    };
    let mut actions = vec![];
    // A new signing key first, or the packages it signed won't verify.
    if list.iter().any(|u| u.name == "archlinux-keyring") {
        actions.push(root(&["pacman", "-Sy", "--needed", "--noconfirm", "archlinux-keyring"]));
    }
    actions.push(upgrade(o));
    let n = plural(list.len(), "package", "packages");
    let mut pl = StagePlan::new(s, Todo::Run(actions), format!("{n} to upgrade"))
        .details(list.iter().map(|u| format!("{} {} → {}", u.name, u.old, u.new)).collect());
    // On a terminal pacman asks "Proceed?" itself; unattended, ask here.
    if !o.interactive {
        pl = pl.ask(format!("Upgrade {n}?"), true);
    }
    pl.packages = list.iter().map(|u| u.name.clone()).collect();
    pl.upgrades = list.clone();
    // AUR builds and release packages expect an upgraded system.
    pl.stops = vec![Stage::Aur, Stage::Releases];
    pl
}

/// `pacman -Syu`: on the terminal it answers its own questions with the
/// user; unattended it takes pacman's default answers (`--noconfirm`), and a
/// question whose default is "no" (a conflict) fails the stage.
fn upgrade(o: &Options) -> Action {
    if o.interactive {
        Action::RootTerminal(vec!["pacman".into(), "-Syu".into()])
    } else {
        root(&["pacman", "-Syu", "--noconfirm"])
    }
}

pub(super) fn aur(p: &Probe, o: &Options) -> StagePlan {
    let s = Stage::Aur;
    let Some(helper) = &p.aur_helper else {
        return StagePlan::new(s, Todo::Skip("no AUR helper".into()), "no AUR helper installed");
    };
    let list = match &p.aur {
        Ok(l) if l.is_empty() => return StagePlan::new(s, Todo::Nothing, "up to date"),
        Ok(l) => l,
        Err(e) => return StagePlan::new(s, Todo::Report, format!("couldn't check the AUR ({e})")),
    };
    let verdict = crate::compat::check(&p.aur_compat, list);
    let waiting: Vec<&str> = verdict.held.iter().map(|h| h.name.as_str()).collect();
    let going: Vec<&Update> = list.iter().filter(|u| !waiting.contains(&u.name.as_str())).collect();
    let mut details: Vec<String> = going.iter().map(|u| format!("{} {} → {}", u.name, u.old, u.new)).collect();
    details.extend(verdict.held.iter().map(|h| format!("held back: {}", h.reason)));
    if let Some(why) = &p.aur_compat.problem {
        details.push(format!("couldn't check partner versions ({why}); pacman still refuses a set that clashes"));
    }
    let held_note = (!waiting.is_empty()).then(|| format!("{} held back until a partner is ready", waiting.len()));
    if going.is_empty() {
        let summary = held_note.unwrap_or_else(|| "up to date".into());
        return StagePlan::new(s, Todo::Report, summary).details(details);
    }
    // Nobody reviews build files in the window or an unattended run.
    let quiet: &[&str] = match (o.yes || o.windowed, helper.as_str()) {
        (false, _) => &[],
        (true, "yay") => &["--noconfirm", "--answerdiff", "None", "--answerclean", "None"],
        (true, _) => &["--noconfirm", "--skipreview"],
    };
    let actions = match &verdict.waves {
        // One run of everything, held ones left out.
        None => {
            let mut argv = vec![helper.clone(), "-Sua".into()];
            argv.extend(quiet.iter().map(|a| a.to_string()));
            if !waiting.is_empty() {
                argv.extend(["--ignore".to_string(), waiting.join(",")]);
            }
            vec![Action::UserElevated(argv)]
        }
        // Partners first: each run only once the one before it went in.
        Some(waves) => waves
            .iter()
            .map(|w| {
                let mut argv = vec![helper.clone(), "-S".into(), "--needed".into()];
                argv.extend(quiet.iter().map(|a| a.to_string()));
                if w.together {
                    argv.push("--batchinstall".into());
                }
                argv.push("--".into());
                argv.extend(w.names.iter().cloned());
                Action::UserElevated(argv)
            })
            .collect(),
    };
    let n = plural(going.len(), "AUR package", "AUR packages");
    let mut summary = format!("{n} to upgrade with {helper}");
    if let Some(note) = held_note {
        summary = format!("{summary}; {note}");
    }
    let mut pl = StagePlan::new(s, Todo::Run(actions), summary).details(details);
    pl.packages = going.iter().map(|u| u.name.clone()).collect();
    pl.upgrades = going.into_iter().cloned().collect();
    pl
}

/// Newer pins update what Bindle installed; a copy from another source
/// stays on it unless the preferences say Bindle's release should win.
/// Bindle's old AppImage copies a package replaced (or whose AppImage is
/// gone) are removed, asked (default yes).
pub(super) fn releases(newer: &[Newer], leftovers: &[Leftover], prefs: &Prefs) -> StagePlan {
    let pl = pinned_releases(newer, prefs);
    if leftovers.is_empty() {
        return pl;
    }
    let names: Vec<String> = leftovers.iter().map(|l| l.name.clone()).collect();
    let names = names.join(", ");
    let mut argv = vec!["rm".to_string(), "-f".into(), "--".into()];
    argv.extend(leftovers.iter().flat_map(|l| l.files.iter().cloned()));
    let clean = Action::Optional {
        question: format!("Remove Bindle's old AppImage copies ({names}) from the app menu and ~/.local/bin?"),
        default: true,
        action: Box::new(Action::User(argv)),
    };
    let note = plural(leftovers.len(), "old AppImage copy to remove", "old AppImage copies to remove");
    let mut details = pl.details.clone();
    details.extend(leftovers.iter().map(|l| {
        let why = if l.replaced { "a package installs it now" } else { "its AppImage is gone" };
        format!("{}: Bindle's old AppImage copy, {why} (removes {})", l.name, l.files.join(", "))
    }));
    let (actions, summary) = match pl.todo {
        Todo::Run(mut actions) => {
            actions.push(clean);
            (actions, format!("{}; {note}", pl.summary))
        }
        _ => (vec![clean], note),
    };
    let packages = pl.packages.clone();
    let mut out = StagePlan::new(Stage::Releases, Todo::Run(actions), summary).details(details);
    out.packages = packages;
    out
}

fn pinned_releases(newer: &[Newer], prefs: &Prefs) -> StagePlan {
    let s = Stage::Releases;
    let (update, kept): (Vec<&Newer>, Vec<&Newer>) =
        newer.iter().partition(|n| n.source == Source::Bindle || prefs.follow(&n.asset.name) == Follow::Bindle);
    let mut details: Vec<String> = update
        .iter()
        .map(|n| {
            let replaces = if n.source == Source::Bindle {
                String::new()
            } else {
                format!(", replacing the copy from {}", n.source.describe())
            };
            format!("{} {} → {} ({}{replaces})", n.asset.name, n.installed, n.asset.tag, n.asset.repo)
        })
        .collect();
    details.extend(kept.iter().map(|n| {
        format!(
            "{}: kept on {} (Bindle pins {}; releases.apps.{} = \"bindle\" switches it)",
            n.asset.name,
            n.source.describe(),
            n.asset.tag,
            n.asset.name
        )
    }));
    let kept_note =
        (!kept.is_empty()).then(|| plural(kept.len(), "app kept on its own source", "apps kept on their own source"));
    if update.is_empty() {
        return match kept_note {
            Some(note) => StagePlan::new(s, Todo::Report, note).details(details),
            None => StagePlan::new(s, Todo::Nothing, "up to date"),
        };
    }
    let mut summary = plural(update.len(), "newer pinned release", "newer pinned releases");
    if let Some(note) = kept_note {
        summary = format!("{summary}; {note}");
    }
    let assets = update.iter().map(|n| n.asset.clone()).collect();
    let mut pl = StagePlan::new(s, Todo::Run(vec![Action::Releases(assets)]), summary).details(details);
    pl.packages = update.iter().map(|n| n.asset.name.clone()).collect();
    pl
}

pub(super) fn flatpak(p: &Probe) -> StagePlan {
    let s = Stage::Flatpak;
    match &p.flatpak {
        None => StagePlan::new(s, Todo::Skip("Flatpak isn't installed".into()), "Flatpak isn't installed"),
        Some(Err(e)) => StagePlan::new(s, Todo::Report, format!("couldn't check ({e})")),
        Some(Ok(l)) if l.is_empty() => StagePlan::new(s, Todo::Nothing, "up to date"),
        Some(Ok(l)) => {
            // Your own installation as you; the system-wide one through the
            // run's root helper (no polkit prompt of Flatpak's own).
            // Runtimes the update leaves unused go too, asked (default yes).
            let clean = Action::Optional {
                question: "Remove Flatpak runtimes no app uses any more?".into(),
                default: true,
                action: Box::new(Action::Each(vec![
                    user(&["flatpak", "uninstall", "--user", "--unused", "-y", "--noninteractive"]),
                    root(&["flatpak", "uninstall", "--system", "--unused", "-y", "--noninteractive"]),
                ])),
            };
            let run = Todo::Run(vec![
                user(&["flatpak", "update", "--user", "-y", "--noninteractive"]),
                root(&["flatpak", "update", "--system", "-y", "--noninteractive"]),
                clean,
            ]);
            StagePlan::new(s, run, format!("{} to update", plural(l.len(), "app or runtime", "apps and runtimes")))
                .details(l.clone())
        }
    }
}

pub(super) fn firmware(p: &Probe, o: &Options) -> StagePlan {
    let pl = firmware_updates(p, o);
    // An old list (a `--check` doesn't refresh it) may miss updates: say so.
    match p.firmware_age {
        Some(age) if p.firmware.is_some() && crate::system::stale(Some(age), o.prefs.firmware_refresh_days) => {
            let mut pl = pl;
            pl.summary.push_str(&format!(" (firmware list {age} days old; an update run refreshes it)"));
            pl
        }
        _ => pl,
    }
}

fn firmware_updates(p: &Probe, o: &Options) -> StagePlan {
    let s = Stage::Firmware;
    match &p.firmware {
        None => StagePlan::new(s, Todo::Skip("fwupd isn't installed".into()), "fwupd isn't installed"),
        Some(Err(e)) => StagePlan::new(s, Todo::Report, format!("couldn't check ({e})")),
        Some(Ok(l)) if l.is_empty() => StagePlan::new(s, Todo::Nothing, "up to date"),
        Some(Ok(l)) if o.firmware => {
            // As root, through the run's helper: fwupd's own polkit prompt is
            // one more password. A terminal run answers its questions itself.
            let action = if o.interactive && !o.yes {
                Action::RootTerminal(vec!["fwupdmgr".into(), "update".into()])
            } else {
                root(&["fwupdmgr", "update", "-y"])
            };
            StagePlan::new(s, Todo::Run(vec![action]), plural(l.len(), "firmware update", "firmware updates"))
                .details(l.clone())
        }
        Some(Ok(l)) => StagePlan::new(
            s,
            Todo::Report,
            if o.windowed {
                plural(l.len(), "update available", "updates available")
            } else {
                format!("{} available; install with --firmware", plural(l.len(), "update", "updates"))
            },
        )
        .details(l.clone()),
    }
}
