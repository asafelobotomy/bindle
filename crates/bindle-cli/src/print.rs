//! Plain-text rendering shared by the CLI and the wizard's review screen.

use bindle_core::{Catalog, Facts, Plan, Step};

pub fn facts(f: &Facts) {
    let yes = |b: bool| if b { "yes" } else { "no" };
    let rows = [
        ("Distro", format!("{} ({})", f.pretty_name, f.distro_id)),
        ("Kernel", f.kernel.clone()),
        ("CPU", format!("{} · {} threads", f.cpu_vendor, f.cpu_cores)),
        ("RAM", format!("{:.1} GiB", f.ram_mib as f64 / 1024.0)),
        ("GPU", if f.gpus.is_empty() { "none".into() } else { f.gpus.join(", ") }),
        ("Root FS", f.root_fs.clone()),
        ("SSD", yes(f.ssd).into()),
        ("Laptop", yes(f.laptop).into()),
        ("Bluetooth", yes(f.bluetooth).into()),
        ("Virtualised", f.virt.clone()),
        ("Bootloader", f.bootloader.clone()),
        ("Session", f.session.clone()),
        ("ZRAM active", yes(f.zram_active).into()),
        ("zswap", f.zswap_enabled.map_or("unknown", yes).into()),
        ("multilib", yes(f.multilib_enabled).into()),
        ("AUR helper", f.aur_helper.clone().unwrap_or_else(|| "none".into())),
        ("Desktops", if f.desktops.is_empty() { "none".into() } else { f.desktops.join(", ") }),
        ("Display mgr", f.display_manager.clone().unwrap_or_else(|| "none".into())),
        ("Packages", f.installed_count.to_string()),
    ];
    for (k, v) in rows {
        println!("{k:>12}  {v}");
    }
}

pub fn list(c: &Catalog, f: &Facts) {
    println!("Profiles:");
    for p in &c.profiles {
        println!("  {:<10} {}", p.id, p.description);
    }
    for cat in &c.categories {
        println!("\n{} — {}", cat.name, cat.description);
        for m in c.modules_in(&cat.id) {
            let status = match f.matches(&m.when) {
                Ok(()) => String::new(),
                Err(r) => format!("  [n/a: {r}]"),
            };
            println!("  {:<18} {}{status}", m.id, m.name);
        }
    }
}

pub fn plan_lines(p: &Plan) -> Vec<String> {
    let mut out = vec![format!("Modules ({}): {}", p.modules.len(), p.modules.join(", "))];
    for (id, why) in &p.provided {
        out.push(format!("  ✓ {id}: {why}"));
    }
    for (dep, by) in &p.pulled_in {
        out.push(format!("  + {dep} (required by {by})"));
    }
    for (id, pkg) in &p.replaced {
        out.push(format!("  ~ {id} replaces installed `{pkg}`"));
    }
    for (id, why) in &p.skipped {
        out.push(format!("  - {id} skipped: {why}"));
    }
    out.push(String::new());
    if p.is_empty() {
        out.push("Nothing to do — this system already matches the selection.".into());
    }
    for (i, step) in p.steps.iter().enumerate() {
        let who = if step.privileged() { "root" } else { "user" };
        out.push(format!("{:>2}. [{who}] {}", i + 1, describe(step)));
        if let Step::WriteFile { file, .. } = step {
            out.extend(file.content.lines().map(|l| format!("        │ {l}")));
        }
    }
    if !p.notes.is_empty() {
        out.push(String::new());
        out.push("Notes:".into());
        out.extend(p.notes.iter().map(|(id, n)| format!("  • {id}: {n}")));
    }
    out
}

pub fn describe(step: &Step) -> String {
    match step {
        Step::EditPacmanConf { options, enable_repos } => {
            let mut parts: Vec<String> =
                options.iter().map(|(k, v)| if v.is_empty() { k.clone() } else { format!("{k}={v}") }).collect();
            parts.extend(enable_repos.iter().map(|r| format!("enable [{r}]")));
            format!("edit /etc/pacman.conf: {}", parts.join(", "))
        }
        Step::RemovePackages { packages } => {
            format!("remove conflicting packages to replace them: {}", packages.join(" "))
        }
        Step::InstallPackages { packages } => {
            format!("full system upgrade, then install ({}): {}", packages.len(), packages.join(" "))
        }
        Step::WriteFile { module, file } => {
            format!("write {} (mode {:o}) [{module}]", file.path, file.mode)
        }
        Step::EnableUnits { units } => format!("systemctl enable {}", units.join(" ")),
        Step::AddToGroups { user, groups } => format!("usermod -aG {} {user}", groups.join(",")),
        Step::RunAsRoot { module, argv } | Step::RunAsUser { module, argv } => {
            format!("{} [{module}]", argv.join(" "))
        }
        Step::RebuildInitramfs => "mkinitcpio -P".into(),
        Step::InstallReleases { assets } => {
            let names: Vec<String> = assets.iter().map(|a| format!("{} {}", a.name, a.tag)).collect();
            format!("install from GitHub releases (checksum-pinned): {}", names.join(", "))
        }
        Step::BootstrapAurHelper { package } => format!("build and install {package} from the AUR"),
        Step::InstallAur { packages } => format!("paru -S --needed {}", packages.join(" ")),
    }
}

/// Summary of what the installer chose, for `bindle archinstall` and the
/// wizard's first screen.
pub fn install_lines(f: &Facts) -> Vec<String> {
    use bindle_core::install::{self, Source};
    let r = &f.install;
    let dash = || "—".to_string();
    let opt = |v: &Option<String>| v.clone().unwrap_or_else(dash);
    let yes = |v: Option<bool>| v.map_or_else(dash, |b| if b { "yes".into() } else { "no".into() });
    let source = match r.source {
        Source::ArchinstallLog => "archinstall's logged config (exact)",
        Source::PacmanLog => "rebuilt from pacman.log (inferred)",
        Source::Unknown => "no installer record found; showing the live system",
    };
    // Short lines: first-login opens in terminals as small as 80×24.
    let mut out = vec![format!("Installed {} by {}", opt(&r.installed_at).get(..10).unwrap_or("—"), opt(&r.installer))];
    out.push(match &r.archinstall_version {
        Some(v) => format!("Source: {source} · archinstall {v}"),
        None => format!("Source: {source}"),
    });
    out.push(String::new());
    let data = bindle_core::desktops::all().unwrap_or_default();
    let names: Vec<String> = f
        .desktops
        .iter()
        .map(|id| bindle_core::desktops::by_id(&data, id).map_or(id.clone(), |d| d.name.clone()))
        .collect();
    let desktops = if names.is_empty() { "no desktop detected".into() } else { names.join(", ") };
    let mut storage = vec![r.root_fs.clone().unwrap_or_else(|| f.root_fs.clone())];
    storage.extend(r.encryption.clone().filter(|e| e != "no_encryption"));
    storage.extend(r.snapshots.clone());
    let greeter = match (&r.greeter, &f.display_manager) {
        // Compare loosely: plasma-login-manager runs as plasmalogin.service.
        (Some(g), Some(dm)) if !g.replace('-', "").starts_with(&dm.replace('-', "")) => format!("{g} (running: {dm})"),
        (g, dm) => g.clone().or(dm.clone()).unwrap_or_else(dash),
    };
    let swap = match (r.zram, &r.zram_algorithm) {
        (Some(true), Some(a)) => format!("zram ({a})"),
        (Some(true), None) => "zram".into(),
        (Some(false), _) => "no zram".into(),
        _ => {
            if f.zram_active {
                "zram (active)".into()
            } else {
                dash()
            }
        }
    };
    let rows = [
        ("Profile", format!("{} · {desktops}", opt(&r.profile))),
        ("Plasma", opt(&r.plasma_flavor)),
        ("Greeter", greeter),
        ("Graphics", opt(&r.gfx_driver)),
        ("Kernels", if r.kernels.is_empty() { dash() } else { r.kernels.join(", ") }),
        ("Bootloader", format!("{}{}", f.bootloader, if r.uki == Some(true) { " · UKI" } else { "" })),
        ("Storage", storage.join(" · ")),
        ("Swap", swap),
        ("Network", opt(&r.network)),
        ("Audio", opt(&r.audio)),
        ("Bluetooth", yes(r.bluetooth)),
        ("Power", opt(&r.power)),
        ("Printing", yes(r.printing)),
        ("Firewall", opt(&r.firewall)),
        ("Fonts", if r.fonts.is_empty() { dash() } else { format!("{} packages", r.fonts.len()) }),
        ("multilib", yes(r.multilib.or(Some(f.multilib_enabled)))),
        (
            "Locale",
            [&r.locale, &r.keyboard, &r.timezone].iter().filter_map(|x| x.as_deref()).collect::<Vec<_>>().join(" · "),
        ),
        ("Extra pkgs", if r.extra_packages.is_empty() { dash() } else { r.extra_packages.join(" ") }),
    ];
    for (k, v) in rows {
        if (k == "Plasma" && r.plasma_flavor.is_none()) || (k == "Locale" && v.is_empty()) {
            continue;
        }
        out.push(format!("  {k:<11} {v}"));
    }
    let provided = r.provided();
    if !provided.is_empty() {
        out.push(String::new());
        out.push("Already set up — Bindle will build on these, not redo them:".into());
        out.extend(provided.iter().map(|(id, why)| format!("  ✓ {id:<16} {why}")));
    }
    let advice = install::advisories(f);
    if !advice.is_empty() {
        out.push(String::new());
        out.push("Suggestions:".into());
        out.extend(advice.iter().map(|a| format!("  • {a}")));
    }
    out
}
