//! Plain-English names for `when` tags, for "not applicable" reasons.

/// "an NVIDIA GPU", "Arch Linux", "a laptop"… for a tag without its `!`.
pub fn label(tag: &str) -> String {
    let Some((prefix, value)) = tag.split_once(':') else {
        return match tag {
            "ssd" => "an SSD",
            "laptop" => "a laptop",
            "bluetooth" => "Bluetooth hardware",
            "virt" => "a virtual machine",
            "multilib" => "multilib enabled",
            "zram-active" => "zram in use",
            "zswap" => "zswap enabled",
            "hibernate" => "hibernation set up",
            "aur-helper" => "an AUR helper",
            "desktop" => "a desktop environment",
            "nm" => "NetworkManager",
            "kdeconnect" => "KDE Connect",
            "snapshots-mount" => "a separate /.snapshots mount",
            "hybrid-gpu" => "hybrid graphics",
            "bare-desktop" => "a desktop without its own everyday apps (uQ, a tiling window manager)",
            "wifi" => "Wi-Fi",
            "thunderbolt" => "Thunderbolt",
            "external-monitor" => "an external monitor",
            "ipu-camera" => "an Intel MIPI camera",
            "fingerprint-reader" => "a fingerprint reader",
            "security-key" => "a smartcard reader or security key",
            "hidpi" => "a high-resolution built-in screen",
            "handheld" => "a handheld gaming PC",
            "asus-gaming" => "an ASUS ROG or TUF laptop",
            "thinkpad-throttle" => "an 8th-generation Intel ThinkPad",
            "charge-limit" => "a battery with a charge limit",
            "pen-tablet" => "a Huion or XP-Pen tablet",
            "usb-epson" => "an Epson device",
            "usb-hp" => "an HP device",
            "ram-8g" => "8 GB of memory or more",
            other => other,
        }
        .into();
    };
    let vendor = |v: &str| match v {
        "nvidia" => "NVIDIA".to_string(),
        "amd" => "AMD".into(),
        "intel" => "Intel".into(),
        other => other.into(),
    };
    match prefix {
        "distro" if value == "arch" => "Arch Linux".into(),
        "distro" => value.into(),
        "cpu" => format!("{} {} CPU", article(value), vendor(value)),
        "gpu" if value == "virtual" => "a virtual GPU".into(),
        "gpu" => format!("{} {} GPU", article(value), vendor(value)),
        "fs" => format!("a {} root filesystem", if value == "btrfs" { "Btrfs" } else { value }),
        "boot" if value == "grub" => "GRUB".into(),
        "boot" => value.into(),
        "session" | "display" => match value {
            "wayland" => "a Wayland session".into(),
            "x11" | "xorg" => "an X11 session".into(),
            other => format!("a {other} session"),
        },
        "toolkit" => match value {
            "qt" => "a Qt desktop (e.g. Plasma)".into(),
            "gtk" => "a GTK desktop (e.g. GNOME)".into(),
            _ => "a desktop that is neither Qt nor GTK".into(),
        },
        "everyday" if value == "qt" => "KDE's everyday apps (uQ, Hyprland)".into(),
        "everyday" => "GNOME's everyday apps (Sway, niri and other window managers)".into(),
        "desktop" => crate::desktops::all()
            .ok()
            .and_then(|d| crate::desktops::by_id(&d, value).map(|d| d.name.clone()))
            .unwrap_or_else(|| value.into()),
        "installer" => format!("an {value} install"),
        "vendor" => format!(
            "a {} computer",
            match value {
                "asus" => "ASUS",
                "lenovo" => "Lenovo",
                "dell" => "Dell",
                "hp" => "HP",
                "framework" => "Framework",
                "system76" => "System76",
                "microsoft" => "Microsoft",
                other => other,
            }
        ),
        "installed" => format!("{value} installed"),
        _ => tag.into(),
    }
}

fn article(v: &str) -> &'static str {
    if matches!(v, "amd" | "intel" | "nvidia" | "other") { "an" } else { "a" }
}

/// Why a module's `when` doesn't hold.
pub fn reason(tag: &str, negated: bool) -> String {
    if negated { format!("not for {}", label(tag)) } else { format!("needs {}", label(tag)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_read_as_english() {
        assert_eq!(reason("distro:arch", false), "needs Arch Linux");
        assert_eq!(reason("virt", true), "not for a virtual machine");
        assert_eq!(reason("gpu:nvidia", false), "needs an NVIDIA GPU");
        assert_eq!(reason("cpu:intel", false), "needs an Intel CPU");
        assert_eq!(reason("fs:btrfs", false), "needs a Btrfs root filesystem");
        assert_eq!(reason("desktop:plasma", false), "needs KDE Plasma");
        assert_eq!(reason("installed:ananicy-cpp", false), "needs ananicy-cpp installed");
        assert_eq!(reason("toolkit:qt", true), "not for a Qt desktop (e.g. Plasma)");
    }
}
