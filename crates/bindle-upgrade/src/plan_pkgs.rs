//! The package stages: pacman, the AUR, Bindle's pinned releases, Flatpak
//! and firmware.

use super::{Action, Options, StagePlan, Todo, plural, root, user};
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
    let mut argv = vec![helper.as_str(), "-Sua"];
    if o.yes {
        argv.extend(match helper.as_str() {
            "yay" => &["--noconfirm", "--answerdiff", "None", "--answerclean", "None"][..],
            _ => &["--noconfirm", "--skipreview"][..],
        });
    }
    let n = plural(list.len(), "AUR package", "AUR packages");
    let details = list.iter().map(|u| format!("{} {} → {}", u.name, u.old, u.new)).collect();
    if o.windowed {
        return StagePlan::new(s, Todo::Report, format!("{n} to upgrade: {helper} needs a terminal")).details(details);
    }
    let mut pl =
        StagePlan::new(s, Todo::Run(vec![user(&argv)]), format!("{n} to upgrade with {helper}")).details(details);
    pl.packages = list.iter().map(|u| u.name.clone()).collect();
    pl
}

/// Newer pins update what Bindle installed; a copy from another source
/// stays on it unless the preferences say Bindle's release should win.
pub(super) fn releases(newer: &[Newer], prefs: &Prefs) -> StagePlan {
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
            // Runtimes the update leaves unused go too, asked (default yes).
            let clean = Action::Optional {
                question: "Remove Flatpak runtimes no app uses any more?".into(),
                default: true,
                action: Box::new(user(&["flatpak", "uninstall", "--unused", "-y", "--noninteractive"])),
            };
            let run = Todo::Run(vec![user(&["flatpak", "update", "-y", "--noninteractive"]), clean]);
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
            let argv: &[&str] = if o.yes { &["fwupdmgr", "update", "-y"] } else { &["fwupdmgr", "update"] };
            StagePlan::new(s, Todo::Run(vec![user(argv)]), plural(l.len(), "firmware update", "firmware updates"))
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
