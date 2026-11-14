//! Which package updates take effect only after a restart: one rule for
//! `bindle apply`, its review screens and bindleupgrade.

/// A kernel, firmware (loaded when a driver starts), microcode, a graphics
/// driver or a core system library. Kernel headers and docs are not.
pub fn needs_restart(name: &str) -> bool {
    (name == "linux" || name.starts_with("linux-")) && !name.ends_with("-headers") && !name.ends_with("-docs")
        || name.ends_with("-ucode")
        || name.starts_with("nvidia")
        || ["mesa", "systemd", "glibc", "dbus", "dbus-broker"].contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernels_drivers_and_core_libraries_need_a_restart() {
        for p in [
            "linux",
            "linux-zen",
            "linux-firmware",
            "linux-lts",
            "amd-ucode",
            "nvidia-open-dkms",
            "nvidia-580xx-dkms",
            "mesa",
            "systemd",
            "glibc",
            "dbus-broker",
        ] {
            assert!(needs_restart(p), "{p}");
        }
        for p in [
            "linux-api-headers",
            "linux-zen-headers",
            "linux-docs",
            "firefox",
            "bash",
            "lib32-mesa-utils",
            "linuxconsole",
        ] {
            assert!(!needs_restart(p), "{p}");
        }
    }
}
