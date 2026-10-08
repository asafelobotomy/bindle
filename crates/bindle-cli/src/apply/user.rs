//! Steps that must run as the invoking user (makepkg refuses root).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::sys::{shell_words, valid_name};

pub fn run(argv: &[String], fake: bool) -> Result<()> {
    let shown = shell_words(argv);
    if fake {
        println!("    would run: {shown}");
        return Ok(());
    }
    let (prog, args) = argv.split_first().context("empty command")?;
    let status = Command::new(prog).args(args).status().with_context(|| format!("start `{prog}`"))?;
    if !status.success() {
        bail!("`{shown}` failed ({status})");
    }
    Ok(())
}

/// Build an AUR package as the user; returns the package files to install.
pub fn build_aur(package: &str, fake: bool) -> Result<Vec<String>> {
    if !valid_name(package) {
        bail!("invalid package name `{package}`");
    }
    let cache = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .context("no cache directory")?
        .join("bindle/aur");
    let dir = cache.join(package);
    let url = format!("https://aur.archlinux.org/{package}.git");
    if fake {
        println!("    would build {package} from {url}");
        return Ok(Vec::new());
    }
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&cache)?;
    let d = dir.to_string_lossy().to_string();
    run(&["git".into(), "clone".into(), "--depth".into(), "1".into(), url, d.clone()], false)?;
    let status = Command::new("makepkg").args(["--noconfirm", "--cleanbuild"]).current_dir(&dir).status()?;
    if !status.success() {
        bail!("makepkg failed for {package} (are base-devel and git installed?)");
    }
    let out = Command::new("makepkg").arg("--packagelist").current_dir(&dir).output()?;
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| std::path::Path::new(l).is_file())
        .map(String::from)
        .collect();
    if files.is_empty() {
        bail!("makepkg produced no package for {package}");
    }
    Ok(files)
}

/// Install AUR packages with the user's AUR helper (it asks sudo itself).
pub fn install_aur(helper: &str, packages: &[String], fake: bool) -> Result<()> {
    if let Some(p) = packages.iter().find(|p| !valid_name(p)) {
        bail!("invalid package name `{p}`");
    }
    let mut argv: Vec<String> = match helper {
        "yay" => ["yay", "-S", "--needed", "--noconfirm", "--answerdiff", "None", "--answerclean", "None"]
            .map(String::from)
            .into(),
        _ => ["paru", "-S", "--needed", "--noconfirm", "--skipreview"].map(String::from).into(),
    };
    argv.extend(packages.iter().cloned());
    run(&argv, fake)
}
