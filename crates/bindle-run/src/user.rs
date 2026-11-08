//! Steps that must run as the invoking user (makepkg refuses root).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use bindle_core::builtins::{AUR_HELPER, AUR_HELPER_COMMIT, AUR_HELPER_PKGBUILD_SHA256};

use crate::sys::{shell_words, valid_name};

pub(crate) fn run(argv: &[String], fake: bool, note: &mut dyn FnMut(&str)) -> Result<()> {
    let shown = shell_words(argv);
    if fake {
        note(&format!("would run: {shown}"));
        return Ok(());
    }
    let (prog, args) = argv.split_first().context("empty command")?;
    let status = Command::new(prog).args(args).status().with_context(|| format!("start `{prog}`"))?;
    if !status.success() {
        bail!("`{shown}` failed ({status})");
    }
    Ok(())
}

/// Build the AUR helper as the user, from the pinned AUR commit; returns the
/// package files for the root helper to install.
pub fn build_aur(package: &str, fake: bool, note: &mut dyn FnMut(&str)) -> Result<Vec<String>> {
    if package != AUR_HELPER {
        bail!("only {AUR_HELPER} is bootstrapped from the AUR, not `{package}`");
    }
    let cache = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .context("no cache directory")?
        .join("bindle/aur");
    let dir = cache.join(package);
    let url = format!("https://aur.archlinux.org/{package}.git");
    if fake {
        note(&format!("would build {package} from {url}"));
        return Ok(Vec::new());
    }
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&cache)?;
    let d = dir.to_string_lossy().to_string();
    run(&["git".into(), "clone".into(), "--quiet".into(), url, d.clone()], false, &mut |_| {})?;
    run(
        &[
            "git".into(),
            "-C".into(),
            d.clone(),
            "checkout".into(),
            "--quiet".into(),
            "--detach".into(),
            AUR_HELPER_COMMIT.into(),
        ],
        false,
        &mut |_| {},
    )
    .context("the pinned AUR commit is gone; Bindle needs an update")?;
    let sum = file_sha256(&dir.join("PKGBUILD"))?;
    if sum != AUR_HELPER_PKGBUILD_SHA256 {
        bail!("{package}'s PKGBUILD doesn't match the pinned checksum (got {sum}); not building it");
    }
    let status = Command::new("makepkg").args(["--noconfirm", "--cleanbuild"]).current_dir(&dir).status()?;
    if !status.success() {
        bail!("makepkg failed for {package} (are base-devel and git installed?)");
    }
    let out = Command::new("makepkg").arg("--packagelist").current_dir(&dir).output()?;
    let files = package_files(&String::from_utf8_lossy(&out.stdout), package);
    if files.is_empty() {
        bail!("makepkg produced no package for {package}");
    }
    Ok(files)
}

/// Built package files from `makepkg --packagelist`, without `-debug` splits.
fn package_files(list: &str, package: &str) -> Vec<String> {
    let debug = format!("{package}-debug-");
    list.lines()
        .filter(|l| Path::new(l).file_name().is_some_and(|n| !n.to_string_lossy().starts_with(&debug)))
        .filter(|l| Path::new(l).is_file())
        .map(String::from)
        .collect()
}

pub(super) fn file_sha256(path: &Path) -> Result<String> {
    let out = Command::new("sha256sum").arg("--").arg(path).output().context("run sha256sum")?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.split_whitespace().next().unwrap_or_default().to_string())
}

/// Install AUR packages with the user's AUR helper (it asks sudo itself).
/// Interactive runs let the helper show PKGBUILDs for review; `--yes` runs
/// can't answer prompts, so they skip the review.
pub fn install_aur(
    helper: &str,
    packages: &[String],
    unattended: bool,
    fake: bool,
    note: &mut dyn FnMut(&str),
) -> Result<()> {
    if let Some(p) = packages.iter().find(|p| !valid_name(p)) {
        bail!("invalid package name `{p}`");
    }
    let mut argv = aur_argv(helper, unattended);
    argv.push("--".into());
    argv.extend(packages.iter().cloned());
    run(&argv, fake, note)
}

fn aur_argv(helper: &str, unattended: bool) -> Vec<String> {
    let args: &[&str] = match (helper, unattended) {
        ("yay", true) => &["yay", "-S", "--needed", "--noconfirm", "--answerdiff", "None", "--answerclean", "None"],
        ("yay", false) => &["yay", "-S", "--needed"],
        (_, true) => &["paru", "-S", "--needed", "--noconfirm", "--skipreview"],
        (_, false) => &["paru", "-S", "--needed"],
    };
    args.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_packages_are_not_installed() {
        let dir = crate::sys::test_dir("aur-list");
        let main = dir.join("paru-bin-2.2.0-1-x86_64.pkg.tar.zst");
        let debug = dir.join("paru-bin-debug-2.2.0-1-x86_64.pkg.tar.zst");
        fs::write(&main, "").unwrap();
        fs::write(&debug, "").unwrap();
        let list = format!("{}\n{}\n{}/missing.pkg.tar.zst\n", main.display(), debug.display(), dir.display());
        assert_eq!(package_files(&list, "paru-bin"), [main.to_string_lossy()]);
    }

    #[test]
    fn interactive_runs_review_pkgbuilds() {
        assert!(!aur_argv("paru", false).iter().any(|a| a == "--skipreview" || a == "--noconfirm"));
        assert!(aur_argv("paru", true).contains(&"--skipreview".to_string()));
        assert!(!aur_argv("yay", false).iter().any(|a| a == "--answerdiff"));
    }

    #[test]
    fn only_the_pinned_helper_is_bootstrapped() {
        assert!(build_aur("evil-bin", true, &mut |_| {}).is_err());
        assert!(build_aur(AUR_HELPER, true, &mut |_| {}).is_ok());
        assert_eq!(AUR_HELPER_COMMIT.len(), 40);
        assert_eq!(AUR_HELPER_PKGBUILD_SHA256.len(), 64);
    }
}
