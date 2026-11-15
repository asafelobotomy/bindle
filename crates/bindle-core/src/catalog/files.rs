//! `[[module.files]]`: a file Bindle writes, its text given inline
//! (`content`) or one of Bindle's own scripts (`source`, from
//! `catalog/files/`, kept as real files so they can be linted and tested).

use serde::{Deserialize, Serialize};

/// The scripts a `source` can name.
const SOURCES: &[(&str, &str)] = &[
    ("bindle-screenshot.sh", include_str!("../../../../catalog/files/bindle-screenshot.sh")),
    ("captive-portal.sh", include_str!("../../../../catalog/files/captive-portal.sh")),
    ("chromium-vaapi.sh", include_str!("../../../../catalog/files/chromium-vaapi.sh")),
    ("copyq-commands.sh", include_str!("../../../../catalog/files/copyq-commands.sh")),
    ("print-key.sh", include_str!("../../../../catalog/files/print-key.sh")),
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(try_from = "RawFileSpec")]
pub struct FileSpec {
    pub path: String,
    pub content: String,
    pub mode: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFileSpec {
    path: String,
    content: Option<String>,
    source: Option<String>,
    #[serde(default = "default_mode")]
    mode: u32,
}

fn default_mode() -> u32 {
    0o644
}

impl TryFrom<RawFileSpec> for FileSpec {
    type Error = String;

    fn try_from(raw: RawFileSpec) -> Result<Self, String> {
        let content = match (raw.content, raw.source) {
            (Some(text), None) => text,
            (None, Some(name)) => match SOURCES.iter().find(|(n, _)| *n == name) {
                Some((_, text)) => (*text).to_string(),
                None => return Err(format!("{}: no script {name} in catalog/files", raw.path)),
            },
            _ => return Err(format!("{}: give either content or source", raw.path)),
        };
        Ok(Self { path: raw.path, content, mode: raw.mode })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(toml: &str) -> Result<FileSpec, toml::de::Error> {
        toml::from_str(toml)
    }

    #[test]
    fn content_or_a_named_script() {
        let f = spec("path = \"/a\"\ncontent = \"x\"\n").unwrap();
        assert_eq!((f.content.as_str(), f.mode), ("x", 0o644));
        let f = spec("path = \"/a\"\nsource = \"print-key.sh\"\nmode = 0o755\n").unwrap();
        assert!(f.content.starts_with("#!/bin/sh\n# Managed by Bindle\n") && f.mode == 0o755);
        assert!(spec("path = \"/a\"\nsource = \"nope.sh\"\n").is_err());
        assert!(spec("path = \"/a\"\n").is_err());
        assert!(spec("path = \"/a\"\ncontent = \"x\"\nsource = \"print-key.sh\"\n").is_err());
    }

    /// Runs a script from SOURCES in `dir` (HOME), returning whether it succeeded.
    fn run(name: &str, dir: &std::path::Path, args: &[&str]) -> bool {
        let text = SOURCES.iter().find(|(n, _)| *n == name).unwrap().1;
        let script = dir.join(name);
        std::fs::write(&script, text).unwrap();
        std::process::Command::new("sh")
            .arg(&script)
            .args(args)
            .env("HOME", dir.join("home"))
            .env_remove("XDG_CONFIG_HOME")
            .env("PRINT_KEY_ROOT", dir.join("root"))
            .status()
            .unwrap()
            .success()
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bindle-scripts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        dir
    }

    #[test]
    fn print_key_adds_once_and_removes_cleanly() {
        let dir = scratch("print-key");
        let default = dir.join("root/usr/share/doc/niri/default-config.kdl");
        std::fs::create_dir_all(default.parent().unwrap()).unwrap();
        let kdl = "binds {\n    Mod+T { spawn \"foot\"; }\n    Print { screenshot; }\n}\n";
        std::fs::write(&default, kdl).unwrap();
        let cfg = dir.join("home/.config/niri/config.kdl");
        assert!(run("print-key.sh", &dir, &["niri"]) && run("print-key.sh", &dir, &["niri"]));
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(text.matches("bindle-screenshot").count(), 1, "{text}");
        assert!(
            text.contains("// Bindle off:     Print { screenshot; }"),
            "the default Print bind is switched off: {text}"
        );
        assert!(run("print-key.sh", &dir, &["niri", "--remove"]));
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), kdl);
        // Hyprland before its first start: nothing to add to yet.
        assert!(!run("print-key.sh", &dir, &["hyprland"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copyq_commands_keep_the_users_own() {
        let dir = scratch("copyq");
        let ini = dir.join("home/.config/copyq/copyq-commands.ini");
        std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
        let mine = "[Commands]\n1\\Command=copyq: popup(1)\n1\\Name=Mine\nsize=1\n";
        std::fs::write(&ini, mine).unwrap();
        assert!(run("copyq-commands.sh", &dir, &[]) && run("copyq-commands.sh", &dir, &[]));
        let text = std::fs::read_to_string(&ini).unwrap();
        assert!(text.contains("1\\Name=Mine") && text.contains("size=6"), "{text}");
        assert_eq!(text.matches("Keep clipboard history: 90 days").count(), 1);
        assert!(run("copyq-commands.sh", &dir, &["--remove"]));
        assert_eq!(std::fs::read_to_string(&ini).unwrap(), mine);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_script_is_managed_by_bindle() {
        for (name, text) in SOURCES {
            assert!(text.starts_with("#!/bin/sh\n# Managed by Bindle\n"), "{name}");
        }
    }
}
