//! `gather`: the whole look at the system. The checks that wait on the
//! network (Bindle's release, the news, the mirrors for pacman, the AUR,
//! Flatpak, fwupd) and the local ones run side by side: a run's start-up is
//! as long as the slowest check, not their sum.

use std::thread::{Scope, ScopedJoinHandle};

use super::*;

/// Look at everything the enabled stages need; `note` hears what is being
/// checked (some checks take a few seconds).
/// `bindle`: the `bindle` program this run belongs to (whether it can update
/// itself depends on where it is). `refresh`: may refresh fwupd's firmware
/// list when it is old (a download into fwupd's cache); a `--check` doesn't.
pub fn gather(
    catalog: &Catalog,
    prefs: &Prefs,
    bindle: &Path,
    refresh: bool,
    enabled: &dyn Fn(Stage) -> bool,
    note: &mut dyn FnMut(&str),
) -> Probe {
    let aur_helper = ["paru", "yay"].into_iter().find(|h| on_path(h)).map(String::from);
    fn off<T>() -> Listed<T> {
        Err("not checked".to_string())
    }
    let has = |st: Stage, ok: bool| enabled(st) && ok;
    for (st, ok, text) in [
        (Stage::Bindle, true, "Checking for a new Bindle"),
        (Stage::News, true, "Reading the Arch news"),
        (Stage::Pacman, true, "Checking for package updates"),
        (Stage::Aur, aur_helper.is_some(), "Checking the AUR"),
        (Stage::Flatpak, on_path("flatpak"), "Checking Flatpak"),
        (Stage::Firmware, on_path("fwupdmgr"), "Checking firmware (fwupd)"),
    ] {
        if has(st, ok) {
            note(text);
        }
    }
    std::thread::scope(|s| {
        // Started first: the slow, network-bound ones.
        let bindle_state = on(s, enabled(Stage::Bindle), || look::bindle(bindle));
        let news = on(s, enabled(Stage::News), || {
            (arch_news(), fs::read_to_string("/var/log/pacman.log").ok().and_then(|l| parse::last_upgrade(&l)))
        });
        let pacman = on(s, enabled(Stage::Pacman), pacman_updates);
        let helper = aur_helper.clone().unwrap_or_default();
        let aur = on(s, has(Stage::Aur, aur_helper.is_some()), move || aur_updates(&helper));
        let flatpak = on(s, enabled(Stage::Flatpak) && on_path("flatpak"), flatpak_updates);
        let firmware = on(s, enabled(Stage::Firmware) && on_path("fwupdmgr"), || {
            let age = look::firmware(prefs.firmware_refresh_days, refresh);
            (age, firmware_updates())
        });
        // The local looks, on this thread meanwhile.
        let installed = installed_versions();
        let mut unchecked = vec![];
        let (newer, problem) =
            if enabled(Stage::Releases) { release_updates(catalog, &installed) } else { (vec![], None) };
        unchecked.extend(problem.map(|why| (Stage::Releases, why)));
        note("Looking at kernels, services and Bindle's setup");
        let mut after = after();
        after.unchecked.extend(unchecked);
        let (news, last_upgrade) = match join(news) {
            Some((n, l)) => (n, l),
            None => (Ok(vec![]), None),
        };
        let (firmware_age, firmware) = match join(firmware) {
            Some((age, f)) => (age, Some(f)),
            None => (None, None),
        };
        Probe {
            today: today(),
            bindle: join(bindle_state).unwrap_or_default(),
            mounts: look::mounts(),
            power: look::power(),
            pin: installed.contains_key("uq-hyprland-pin").then(look::pin),
            pacman: join(pacman).unwrap_or_else(off::<Update>),
            aur: join(aur).unwrap_or_else(off::<Update>),
            aur_helper,
            flatpak: join(flatpak),
            firmware,
            firmware_age,
            reflector: on_path("reflector"),
            mirrorlist_age_days: age_days(Path::new("/etc/pacman.d/mirrorlist")),
            news,
            last_upgrade,
            releases: newer,
            after,
            snapper_root: Path::new("/etc/snapper/configs/root").is_file(),
            snap_pac: installed.contains_key("snap-pac"),
        }
    })
}

/// Start `f` on its own thread when `want`.
pub(super) fn on<'s, 'e, T: Send + 's>(
    s: &'s Scope<'s, 'e>,
    want: bool,
    f: impl FnOnce() -> T + Send + 's,
) -> Option<ScopedJoinHandle<'s, T>> {
    want.then(|| s.spawn(f))
}

/// What a started check found; None when it wasn't started (or panicked).
pub(super) fn join<T>(h: Option<ScopedJoinHandle<'_, T>>) -> Option<T> {
    h?.join().ok()
}
