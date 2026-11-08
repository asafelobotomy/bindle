//! Plain-text output for the CLI; the rendering shared with other frontends
//! lives in `bindle_run::present`.

use bindle_core::{Catalog, Facts};

pub use bindle_run::present::{describe, install_lines, module_details, plan_lines};

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
