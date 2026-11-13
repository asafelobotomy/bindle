//! The stages that close a run, planned from a look at the system after
//! the upgrades: rebuilds, uQ, config files and orphans.

use super::{Action, Options, StagePlan, Todo, plural, root, user};
use crate::probe::After;
use crate::stage::Stage;

pub(super) fn pacfiles(files: &[String]) -> StagePlan {
    let s = Stage::Pacfiles;
    if files.is_empty() {
        return StagePlan::new(s, Todo::Nothing, "no .pacnew or .pacsave files");
    }
    let n = plural(files.len(), "file", "files");
    StagePlan::new(s, Todo::Report, format!("{n} to merge: run `bindo pacnew`")).details(files.to_vec())
}

pub(super) fn orphans(list: &[String], o: &Options) -> StagePlan {
    let s = Stage::Orphans;
    if list.is_empty() {
        return StagePlan::new(s, Todo::Nothing, "none");
    }
    let n = plural(list.len(), "orphaned package", "orphaned packages");
    let mut argv = vec!["pacman".to_string(), "-Rns".into(), "--noconfirm".into(), "--".into()];
    argv.extend(list.iter().cloned());
    let run = Todo::Run(vec![Action::Root(argv)]);
    if o.remove_orphans {
        StagePlan::new(s, run, format!("removing {n}")).details(list.to_vec())
    } else if !o.yes {
        StagePlan::new(s, run, n.clone()).details(list.to_vec()).ask(format!("Remove {n}?"), false)
    } else {
        StagePlan::new(s, Todo::Report, format!("{n}; remove with --remove-orphans or `bindo orphans`"))
            .details(list.to_vec())
    }
}

/// AUR and other foreign packages a library update broke (rebuild-detector),
/// rebuilt with the AUR helper.
pub(super) fn rebuild(now: &After, updated: &[String], o: &Options, pending: bool) -> StagePlan {
    let s = Stage::Rebuild;
    let Some(list) = &now.rebuild else {
        return match &now.aur_helper {
            Some(_) if !updated.is_empty() => {
                StagePlan::new(s, Todo::Report, "install rebuild-detector to find AUR packages a library update broke")
                    .details(vec!["pkexec pacman -S rebuild-detector".into()])
            }
            _ => StagePlan::new(
                s,
                Todo::Skip("rebuild-detector isn't installed".into()),
                "rebuild-detector isn't installed",
            ),
        };
    };
    let foreign: Vec<String> = list.iter().filter(|r| r.repo == "foreign").map(|r| r.name.clone()).collect();
    let mut details: Vec<String> = foreign.clone();
    details.extend(
        list.iter().filter(|r| r.repo != "foreign").map(|r| format!("{} (from {}: reinstall it)", r.name, r.repo)),
    );
    if list.is_empty() {
        return StagePlan::new(s, Todo::Nothing, "nothing needs a rebuild");
    }
    let n = plural(foreign.len(), "AUR package needs", "AUR packages need");
    let Some(helper) = now.aur_helper.as_deref().filter(|_| !foreign.is_empty()) else {
        return StagePlan::new(
            s,
            Todo::Report,
            format!("{} need attention", plural(list.len(), "package", "packages")),
        )
        .details(details);
    };
    if pending || o.windowed {
        let how = if o.windowed { "in a terminal" } else { "after the upgrade" };
        return StagePlan::new(s, Todo::Report, format!("{n} a rebuild: {helper} rebuilds them {how}"))
            .details(details);
    }
    let mut argv = vec![helper, "-S", "--rebuild"];
    if o.yes {
        argv.extend(if helper == "yay" {
            &["--noconfirm", "--answerdiff", "None"][..]
        } else {
            &["--noconfirm", "--skipreview"][..]
        });
    }
    argv.push("--");
    let mut action = user(&argv);
    if let Action::User(v) = &mut action {
        v.extend(foreign.iter().cloned());
    }
    StagePlan::new(s, Todo::Run(vec![action]), format!("{n} a rebuild after the library updates"))
        .details(details)
        .ask(format!("Rebuild {}?", plural(foreign.len(), "AUR package", "AUR packages")), true)
}

/// uQ was upgraded: its per-user setup again (`uqctl setup --apps`, which
/// never overwrites the user's own files) and its login screen kept on. Not
/// `uqctl reload`: that asks the old uqd, still running, to render the new
/// templates; the new uQ starts at the next login.
pub(super) fn uq(before: &After, now: &After, updated: &[String], pending: bool) -> StagePlan {
    let s = Stage::Uq;
    if !now.uq {
        return StagePlan::new(s, Todo::Skip("uQ isn't installed".into()), "uQ isn't installed");
    }
    let upgraded = updated.iter().any(|u| u == "uq");
    let greeter_lost = before.greeter && !now.greeter;
    if pending {
        return match upgraded {
            true => {
                StagePlan::new(s, Todo::Report, "uQ updates: its setup runs again afterwards (`uqctl setup --apps`)")
            }
            false => StagePlan::new(s, Todo::Nothing, "no uQ update"),
        };
    }
    let mut actions = vec![];
    let mut what = vec![];
    if upgraded {
        actions.push(Action::UqSetup);
        what.push("set up its files again");
    }
    if greeter_lost {
        actions.push(root(&["uqctl", "greeter", "enable"]));
        what.push("turn its login screen back on");
    }
    if actions.is_empty() {
        return StagePlan::new(s, Todo::Nothing, "no uQ update");
    }
    let mut summary = format!("uQ: {}", what.join(", "));
    if upgraded {
        summary.push_str("; log out and back in to start the new uQ");
    }
    StagePlan::new(s, Todo::Run(actions), summary)
}
