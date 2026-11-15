//! Hardware modules whose files depend on what was found: kernel headers for
//! DKMS modules (one per installed kernel) and the battery's charge limit
//! (each battery's own sysfs file).

use super::Expansion;
use crate::catalog::FileSpec;
use crate::detect::Facts;

pub const CHARGE_LIMIT_FILE: &str = "/etc/tmpfiles.d/bindle-charge-limit.conf";
/// Where charging stops, in percent.
pub const CHARGE_LIMIT: u8 = 80;

/// Headers for every installed kernel, so a DKMS module builds for each.
pub(super) fn kernel_headers(f: &Facts) -> Expansion {
    Expansion { packages: super::gpu::kernel_headers(f), ..Default::default() }
}

/// Stop charging at CHARGE_LIMIT% (IdeaPads: conservation mode, about 60%),
/// set again at every start by systemd-tmpfiles.
pub(super) fn charge_limit(f: &Facts) -> Expansion {
    let mut lines = vec!["# Managed by Bindle: stop charging early, for a longer-lived battery".to_string()];
    for path in &f.devices.charge_limit {
        let value = if path.ends_with("conservation_mode") { "1".to_string() } else { CHARGE_LIMIT.to_string() };
        lines.push(format!("w {path} - - - - {value}"));
    }
    if lines.len() == 1 {
        return Expansion::default();
    }
    let content = lines.join("\n") + "\n";
    Expansion {
        files: vec![FileSpec { path: CHARGE_LIMIT_FILE.into(), content, mode: 0o644 }],
        notes: vec![format!(
            "Charging stops at {CHARGE_LIMIT}% from the next start (to charge fully once, remove {CHARGE_LIMIT_FILE} and restart)."
        )],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_battery_gets_its_limit() {
        let mut f = Facts::default();
        assert!(charge_limit(&f).files.is_empty());
        f.devices.charge_limit = vec![
            "/sys/class/power_supply/BAT0/charge_control_end_threshold".into(),
            "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode".into(),
        ];
        let c = &charge_limit(&f).files[0].content;
        assert!(c.contains("w /sys/class/power_supply/BAT0/charge_control_end_threshold - - - - 80\n"));
        assert!(c.contains("conservation_mode - - - - 1\n"));
    }
}
