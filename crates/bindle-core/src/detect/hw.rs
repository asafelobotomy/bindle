//! CPU, memory, GPU, storage, power and virtualisation.

use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

use super::read;

pub fn cpu_vendor(cpuinfo: &str) -> String {
    let id = cpuinfo.lines().find(|l| l.starts_with("vendor_id")).unwrap_or_default();
    if id.contains("GenuineIntel") {
        "intel".into()
    } else if id.contains("AuthenticAMD") {
        "amd".into()
    } else {
        "other".into()
    }
}

pub fn mem_total_mib(meminfo: &str) -> u64 {
    meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(0, |kib| kib / 1024)
}

pub fn pci_vendor_name(id: &str) -> &'static str {
    match id.trim() {
        "0x10de" => "nvidia",
        "0x1002" => "amd",
        "0x8086" => "intel",
        // virtio, VMware, Bochs/QEMU stdvga, VirtualBox, QEMU/Red Hat (QXL, virtio-gpu), Hyper-V.
        "0x1af4" | "0x15ad" | "0x1234" | "0x80ee" | "0x1b36" | "0x1414" => "virtual",
        _ => "other",
    }
}

/// First PCI device id of the Turing generation (TU102); nvidia-open needs
/// Turing or newer. Pascal/Volta ids end below this (GV100 is 0x1dbx).
pub const TURING_FIRST_DEVICE: u16 = 0x1e00;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct GpuInfo {
    pub vendors: Vec<String>,
    pub nvidia_devices: Vec<u16>,
}

fn hex_id(s: &str) -> Option<u16> {
    u16::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

/// `cards` holds (vendor, device) sysfs strings per DRM card. Cards without a
/// vendor (simpledrm/efifb) are ignored; in a VM an unrecognised adapter is
/// reported as `virtual`.
pub fn classify_gpus(cards: &[(Option<String>, Option<String>)], virt: &str) -> GpuInfo {
    let mut vendors = BTreeSet::new();
    let mut nvidia_devices = Vec::new();
    for (vendor, device) in cards {
        let Some(vendor) = vendor.as_deref().filter(|v| !v.trim().is_empty()) else { continue };
        let name = pci_vendor_name(vendor);
        if name == "nvidia"
            && let Some(id) = device.as_deref().and_then(hex_id)
            && !nvidia_devices.contains(&id)
        {
            nvidia_devices.push(id);
        }
        vendors.insert(name.to_string());
    }
    if virt != "none" && !virt.is_empty() && vendors.len() == 1 && vendors.contains("other") {
        vendors = BTreeSet::from(["virtual".to_string()]);
    }
    GpuInfo { vendors: vendors.into_iter().collect(), nvidia_devices }
}

pub(super) fn gpus(virt: &str) -> GpuInfo {
    let mut cards = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("card") && !name.contains('-') {
                let attr = |a: &str| fs::read_to_string(format!("/sys/class/drm/{name}/device/{a}")).ok();
                cards.push((attr("vendor"), attr("device")));
            }
        }
    }
    classify_gpus(&cards, virt)
}

pub(super) fn has_ssd() -> bool {
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let real = ["nvme", "sd", "vd", "mmcblk"].iter().any(|p| name.starts_with(p));
        // A USB stick or SD card isn't the system's disk.
        let removable = read(&format!("/sys/block/{name}/removable")).trim() == "1";
        real && !removable && read(&format!("/sys/block/{name}/queue/rotational")).trim() == "0"
    })
}

/// SMBIOS chassis types for portables: Portable, Laptop, Notebook,
/// Sub Notebook, Tablet, Convertible.
/// SMBIOS chassis types: portable, laptop, notebook, sub-notebook, tablet,
/// convertible, detachable.
const PORTABLE_CHASSIS: &[&str] = &["8", "9", "10", "14", "30", "31", "32"];

/// Laptop-like: a portable chassis, or a system battery. `supplies` holds
/// (type, scope) per power_supply; `scope=Device` batteries belong to mice,
/// headsets and controllers.
pub fn is_portable(chassis_type: &str, supplies: &[(String, String)]) -> bool {
    PORTABLE_CHASSIS.contains(&chassis_type.trim())
        || supplies.iter().any(|(kind, scope)| kind.trim() == "Battery" && scope.trim() != "Device")
}

pub(super) fn has_battery() -> bool {
    let supplies: Vec<(String, String)> = fs::read_dir("/sys/class/power_supply")
        .map(|d| {
            d.flatten()
                .map(|e| {
                    let p = e.path().display().to_string();
                    (read(&format!("{p}/type")), read(&format!("{p}/scope")))
                })
                .collect()
        })
        .unwrap_or_default();
    is_portable(&read("/sys/class/dmi/id/chassis_type"), &supplies)
}

pub(super) fn virt() -> String {
    Command::new("systemd-detect-virt")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(v: &str, d: &str) -> (Option<String>, Option<String>) {
        (Some(format!("{v}\n")), Some(format!("{d}\n")))
    }

    #[test]
    fn parses_proc_files() {
        assert_eq!(cpu_vendor("vendor_id\t: AuthenticAMD\n"), "amd");
        assert_eq!(mem_total_mib("MemTotal:       16303428 kB\n"), 15921);
    }

    #[test]
    fn gpus_skip_vendorless_and_record_nvidia_ids() {
        let cards = [card("0x8086", "0x9a49"), card("0x10de", "0x1c8d"), (None, None)];
        let g = classify_gpus(&cards, "none");
        assert_eq!(g.vendors, ["intel", "nvidia"]);
        assert_eq!(g.nvidia_devices, [0x1c8d]);
    }

    #[test]
    fn unknown_adapter_in_a_vm_is_virtual() {
        assert_eq!(classify_gpus(&[card("0x1b36", "0x0100")], "kvm").vendors, ["virtual"]);
        assert_eq!(classify_gpus(&[card("0x1414", "0x5353")], "microsoft").vendors, ["virtual"]);
        assert_eq!(classify_gpus(&[card("0x1013", "0x00b8")], "kvm").vendors, ["virtual"]);
        assert_eq!(classify_gpus(&[card("0x1013", "0x00b8")], "none").vendors, ["other"]);
    }

    #[test]
    fn portable_detection() {
        let s = |k: &str, sc: &str| (k.to_string(), sc.to_string());
        assert!(is_portable("10\n", &[]));
        assert!(!is_portable("3", &[s("Battery", "Device"), s("Mains", "")]));
        assert!(is_portable("3", &[s("Battery\n", "System\n")]));
        assert!(is_portable("", &[s("Battery", "")]));
    }
}
