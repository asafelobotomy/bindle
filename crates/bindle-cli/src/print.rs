//! Plain-text output for the CLI; the rendering shared with other frontends
//! lives in `bindle_run::present`.

use bindle_core::{Catalog, Facts};

pub use bindle_run::present::{describe, install_lines, module_details, plan_lines};

pub fn facts(f: &Facts) {
    let yes = |b: bool| if b { "yes" } else { "no" };
    let drives: Vec<String> =
        f.drives.iter().map(|d| format!("{} {} {}", d.device, d.fstype, d.label).trim_end().to_string()).collect();
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
        ("Other drives", if f.drives.is_empty() { "none".into() } else { drives.join("; ") }),
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

/// `bindle list | head` closes stdout early; `println!` then panics (and the
/// release build aborts). Treat that like SIGPIPE would: quietly stop.
pub fn quiet_broken_pipe() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or_default();
        if msg.starts_with("failed printing to std") && msg.contains("Broken pipe") {
            std::process::exit(0);
        }
        default(info);
    }));
}
