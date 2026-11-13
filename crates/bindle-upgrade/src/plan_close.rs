//! The last checks of a run: services that failed since it started, Bindle's
//! own setup, and whether to restart, and whether that is safe yet.

use super::{Action, StagePlan, Todo, plural};
use crate::probe::After;
use crate::setup::Setup;
use crate::stage::Stage;
use crate::system::{dkms_missing, images_missing};
use bindle_core::whatsnew::Kind;

/// Units failed now that weren't before the run (`--check`: those failed now).
pub(super) fn services(before: &After, now: &After, pending: bool) -> StagePlan {
    let s = Stage::Services;
    let new: Vec<&String> = now.failed.iter().filter(|u| pending || !before.failed.contains(u)).collect();
    let old: Vec<&String> = now.failed.iter().filter(|u| !new.contains(u)).collect();
    if new.is_empty() {
        let summary = if old.is_empty() {
            "none failed".to_string()
        } else {
            format!("{} failed before the update too", plural(old.len(), "service", "services"))
        };
        return StagePlan::new(s, Todo::Nothing, summary);
    }
    let what = if pending { "failed now" } else { "failed since the update" };
    let mut details: Vec<String> = new.iter().map(|u| format!("{u}: systemctl status {u}")).collect();
    details.extend(old.iter().map(|u| format!("{u} (failed before the update too)")));
    StagePlan::new(s, Todo::Report, format!("{} {what}", plural(new.len(), "service", "services"))).details(details)
}

/// What's new (this Bindle compared with what earlier versions set up),
/// and what changed since Bindle set it up. Reported only: `bindle
/// whats-new` or the window reviews and applies.
pub(super) fn setup(st: &Setup) -> StagePlan {
    let s = Stage::Setup;
    let mut parts = vec![];
    if !st.whats_new.is_empty() {
        let n = plural(st.whats_new.len(), "item", "items");
        parts.push(format!("{n} in What's new: review them with `bindle whats-new`"));
    }
    if !st.drift.is_empty() {
        parts.push(format!("{} since Bindle set it up", plural(st.drift.len(), "change", "changes")));
    }
    if parts.is_empty() {
        let summary = if st.known { "as Bindle set it up" } else { "nothing from Bindle to compare" };
        return StagePlan::new(s, Todo::Nothing, summary);
    }
    let word = |k: Kind| match k {
        Kind::New => "new",
        Kind::Optional => "optional",
        Kind::Updated => "changed",
        Kind::Retired => "retired",
        Kind::Removed => "removed by you",
    };
    let mut details: Vec<String> = st.whats_new.iter().map(|(k, name)| format!("{}: {name}", word(*k))).collect();
    details.extend(st.drift.iter().cloned());
    StagePlan::new(s, Todo::Report, parts.join("; ")).details(details)
}

/// Packages whose update takes effect only after a restart.
pub fn needs_restart(name: &str) -> bool {
    (name == "linux" || name.starts_with("linux-")) && name != "linux-api-headers"
        || name.ends_with("-ucode")
        || name.starts_with("nvidia")
        || ["mesa", "systemd", "glibc", "dbus", "dbus-broker"].contains(&name)
}

/// Whether to restart, held ("don't restart yet") when a DKMS module isn't
/// built for an installed kernel or a boot image is missing, with the fix.
pub(super) fn reboot(a: &After, updated: &[String], pending: bool) -> StagePlan {
    let s = Stage::Reboot;
    let mut why = vec![];
    if !a.kernel_modules_present {
        why.push(format!("the running kernel ({}) was replaced", a.running_kernel));
    }
    let hits: Vec<&str> = updated.iter().map(String::as_str).filter(|n| needs_restart(n)).collect();
    if !hits.is_empty() {
        let verb = if pending { "will update" } else { "updated" };
        why.push(format!("{verb} {}", hits.join(", ")));
    }
    let dkms = dkms_missing(&a.kernels, &a.dkms);
    let images = a.boot.as_ref().map(|b| images_missing(&a.kernels, b)).unwrap_or_default();
    let mut broken: Vec<String> =
        dkms.iter().map(|(m, k)| format!("{m} isn't built for {k}: pkexec dkms autoinstall -k {k}")).collect();
    broken.extend(images.iter().map(|f| format!("/boot/{f} is missing: pkexec mkinitcpio -P")));
    if !broken.is_empty() {
        let mut details = broken.clone();
        details.extend(why.iter().map(|w| format!("restart afterwards: {w}")));
        let hold = format!("don't restart yet: {}", broken[0]);
        if pending {
            return StagePlan::new(s, Todo::Hold(hold.clone()), hold).details(details);
        }
        // After the upgrade: build what is missing (DKMS first: the images may include its modules).
        let mut kernels: Vec<&str> = dkms.iter().map(|(_, k)| k.as_str()).collect();
        kernels.dedup();
        let mut actions: Vec<Action> =
            kernels.iter().map(|k| Action::Root(["dkms", "autoinstall", "-k", k].map(String::from).to_vec())).collect();
        actions.push(Action::Root(vec!["mkinitcpio".into(), "-P".into()]));
        return StagePlan::new(s, Todo::Run(actions), format!("{hold}; building what is missing"))
            .details(details)
            .ask("Build the missing modules and boot images now?", true);
    }
    let mut details = vec![];
    if a.boot.is_none() && !a.kernels.is_empty() {
        details.push("boot images not checked: /boot can only be read as root".to_string());
    }
    if why.is_empty() {
        return StagePlan::new(s, Todo::Nothing, "not needed").details(details);
    }
    let when = if pending && a.kernel_modules_present { "will be needed" } else { "recommended" };
    StagePlan::new(s, Todo::Report, format!("{when}: {}", why.join("; "))).details(details)
}
