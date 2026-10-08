//! Which bootloader this system uses.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Name of the running loader from the Boot Loader Interface `LoaderInfo`
/// EFI variable (set by systemd-boot and Limine; world-readable).
pub fn loader_from_info(info: &str) -> Option<&'static str> {
    let info = info.to_ascii_lowercase();
    if info.contains("systemd-boot") {
        Some("systemd-boot")
    } else if info.contains("limine") {
        Some("limine")
    } else if info.contains("grub") {
        Some("grub")
    } else {
        None
    }
}

fn loader_info() -> Option<String> {
    let dir = fs::read_dir("/sys/firmware/efi/efivars").ok()?;
    let entry = dir.flatten().find(|e| e.file_name().to_string_lossy().starts_with("LoaderInfo-"))?;
    let raw = fs::read(entry.path()).ok()?;
    // 4-byte attribute header, then UTF-16LE.
    let (pairs, _) = raw.get(4..)?.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|&c| u16::from_le_bytes(c)).take_while(|&u| u != 0).collect();
    Some(String::from_utf16_lossy(&units))
}

const SYSTEMD_BOOT_FILES: &[&str] =
    &["/boot/loader/loader.conf", "/efi/loader/loader.conf", "/boot/efi/loader/loader.conf"];
const LIMINE_FILES: &[&str] = &[
    "/boot/limine.conf",
    "/boot/limine/limine.conf",
    "/boot/EFI/limine/limine.conf",
    "/efi/EFI/limine/limine.conf",
    "/boot/efi/EFI/limine/limine.conf",
];

/// LoaderInfo first, then config files, then installed loader packages (the
/// ESP is often root-only, so its files can be invisible to us). Only
/// systemd-boot and Limine set LoaderInfo, so a package hint is safe after it.
pub fn bootloader_from(info: Option<&str>, exists: &dyn Fn(&str) -> bool, installed: &BTreeSet<String>) -> String {
    if let Some(name) = info.and_then(loader_from_info) {
        return name.into();
    }
    let any = |paths: &[&str]| paths.iter().any(|p| exists(p));
    if any(SYSTEMD_BOOT_FILES) {
        "systemd-boot".into()
    } else if any(LIMINE_FILES) {
        "limine".into()
    } else if exists("/boot/grub/grub.cfg") {
        "grub".into()
    } else if exists("/boot/refind_linux.conf") {
        "refind".into()
    } else if let Some(pkg) = ["grub", "limine", "refind"].into_iter().find(|p| installed.contains(*p)) {
        pkg.into()
    } else {
        "unknown".into()
    }
}

pub(super) fn bootloader(installed: &BTreeSet<String>) -> String {
    bootloader_from(loader_info().as_deref(), &|p| Path::new(p).exists(), installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_info_names() {
        assert_eq!(loader_from_info("systemd-boot 261.2-1-arch"), Some("systemd-boot"));
        assert_eq!(loader_from_info("Limine 9.3"), Some("limine"));
    }

    #[test]
    fn falls_back_to_files_then_packages() {
        let none = BTreeSet::new();
        let pkgs = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        let no_files = |_: &str| false;
        assert_eq!(bootloader_from(Some("Limine 9.3"), &no_files, &pkgs(&["grub"])), "limine");
        assert_eq!(bootloader_from(None, &|p| p == "/boot/EFI/limine/limine.conf", &none), "limine");
        assert_eq!(bootloader_from(None, &no_files, &pkgs(&["grub", "efibootmgr"])), "grub");
        assert_eq!(bootloader_from(None, &no_files, &pkgs(&["refind"])), "refind");
        assert_eq!(bootloader_from(None, &no_files, &none), "unknown");
    }
}
