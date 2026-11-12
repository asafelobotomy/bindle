//! Steps that must run as the invoking user (makepkg refuses root).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use bindle_core::builtins::{AUR_HELPER, AUR_HELPER_COMMIT, AUR_HELPER_PKGBUILD_SHA256};

use crate::sys::{shell_words, valid_name};

pub(crate) fn run(argv: &[String], fake: bool, note: &mut dyn FnMut(&str)) -> Result<()> {
    run_command(argv, &BTreeMap::new(), fake, note).map(drop)
}

/// A catalog user command. An exit code listed in `manual` isn't a failure:
/// its message (what the user still has to do) is returned instead.
pub(crate) fn run_command(
    argv: &[String],
    manual: &BTreeMap<String, String>,
    fake: bool,
    note: &mut dyn FnMut(&str),
) -> Result<Option<String>> {
    let shown = shell_words(argv);
    if fake {
        note(&format!("would run: {shown}"));
        return Ok(None);
    }
    let (prog, args) = argv.split_first().context("empty command")?;
    let status = Command::new(prog).args(args).status().with_context(|| format!("start `{prog}`"))?;
    exit_outcome(&shown, status.success(), status.code(), manual, &status.to_string())
}

fn exit_outcome(
    shown: &str,
    success: bool,
    code: Option<i32>,
    manual: &BTreeMap<String, String>,
    status: &str,
) -> Result<Option<String>> {
    if success {
        return Ok(None);
    }
    match code.and_then(|c| manual.get(&c.to_string())) {
        Some(todo) => Ok(Some(todo.clone())),
        None => bail!("`{shown}` failed ({status})"),
    }
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
    if fake {
        note(&format!("would build {package} from https://aur.archlinux.org/{package}.git"));
        return Ok(Vec::new());
    }
    fs::create_dir_all(&cache)?;
    let d = dir.to_string_lossy().to_string();
    clone_aur(package, &dir, note)?;
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

/// Where a package's AUR git history can be cloned from, in order: the AUR,
/// then Arch's GitHub copy of it (one branch per package), which carries the
/// same commits, so the pinned commit and PKGBUILD checksum still decide what
/// gets built.
fn aur_sources(package: &str) -> Vec<Vec<String>> {
    let aur = vec![format!("https://aur.archlinux.org/{package}.git")];
    let github = vec![
        "--branch".into(),
        package.to_string(),
        "--single-branch".into(),
        "https://github.com/archlinux/aur.git".into(),
    ];
    vec![aur.clone(), aur, github]
}

/// Clone with retries: the AUR often drops connections when it is busy.
fn clone_aur(package: &str, dir: &Path, note: &mut dyn FnMut(&str)) -> Result<()> {
    let sources = aur_sources(package);
    let mut last = None;
    for (i, src) in sources.iter().enumerate() {
        if i > 0 {
            let to = if i + 1 == sources.len() { "Arch's GitHub copy of the AUR" } else { "the AUR again" };
            note(&format!("couldn't download {package} from the AUR; trying {to}"));
            std::thread::sleep(std::time::Duration::from_secs(3 * i as u64));
        }
        let _ = fs::remove_dir_all(dir);
        let mut argv: Vec<String> = vec!["git".into(), "clone".into(), "--quiet".into()];
        argv.extend(src.iter().cloned());
        argv.push(dir.to_string_lossy().into_owned());
        match run(&argv, false, &mut |_| {}) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("no source to clone {package} from"))).context(format!(
        "couldn't download {package} from the AUR or its GitHub copy; check the network and run Bindle again"
    ))
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
    // The helper downloads from the AUR, which drops connections when busy.
    // `--needed` makes a second try pick up where the first stopped.
    match run(&argv, fake, note) {
        Ok(()) => Ok(()),
        Err(e) => {
            note(&format!("{e}; trying the AUR install once more"));
            std::thread::sleep(std::time::Duration::from_secs(5));
            run(&argv, fake, note)
        }
    }
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
    fn manual_exit_codes_are_reported_not_failed() {
        let manual = BTreeMap::from([("3".to_string(), "add the stub".to_string())]);
        assert_eq!(exit_outcome("x", true, Some(0), &manual, "").unwrap(), None);
        assert_eq!(exit_outcome("x", false, Some(3), &manual, "").unwrap().as_deref(), Some("add the stub"));
        let e = exit_outcome("uqctl setup", false, Some(1), &manual, "exit status: 1").unwrap_err();
        assert!(e.to_string().contains("`uqctl setup` failed"));
        assert!(exit_outcome("x", false, None, &manual, "signal 9").is_err());
    }

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
    fn aur_clone_falls_back_to_the_github_copy() {
        let s = aur_sources("paru-bin");
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], ["https://aur.archlinux.org/paru-bin.git"]);
        assert_eq!(s[1], s[0]);
        assert_eq!(s[2], ["--branch", "paru-bin", "--single-branch", "https://github.com/archlinux/aur.git"]);
    }

    #[test]
    fn only_the_pinned_helper_is_bootstrapped() {
        assert!(build_aur("evil-bin", true, &mut |_| {}).is_err());
        assert!(build_aur(AUR_HELPER, true, &mut |_| {}).is_ok());
        assert_eq!(AUR_HELPER_COMMIT.len(), 40);
        assert_eq!(AUR_HELPER_PKGBUILD_SHA256.len(), 64);
    }
}
