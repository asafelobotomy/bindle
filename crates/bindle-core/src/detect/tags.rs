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
        "desktop" => crate::desktops::all()
            .ok()
            .and_then(|d| crate::desktops::by_id(&d, value).map(|d| d.name.clone()))
            .unwrap_or_else(|| value.into()),
        "installer" => format!("an {value} install"),
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
        assert_eq!(reason("toolkit:qt", true), "not for a Qt desktop (e.g. Plasma)");
    }
}
