//! `bindle restore`: Factory Restore. Gets the Arch ISO (downloaded, or the
//! copy kept on this computer when there's no internet), has the root
//! helper check it and prepare the next start, then restarts into the
//! installer. Nothing is erased until the installer's own last question.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use bindle_core::recover::restore::IMAGE;
use bindle_run::client::Launch;
use bindle_run::protocol::Request;
use bindle_run::session::Session;
use bindle_run::sys;

const MIRROR: &str = "https://geo.mirror.pkgbuild.com/iso/latest";
/// Testing (the VM test serves the ISO it has): another place to download
/// from. Safe to point anywhere: the root helper checks the signature.
const MIRROR_ENV: &str = "BINDLE_ISO_MIRROR";
const ISO: &str = "archlinux-x86_64.iso";

#[derive(clap::Args, Debug)]
pub struct RestoreArgs {
    /// Keep a checked copy of the ISO on this computer (on), for Factory
    /// Restore without the internet, or delete it (off). Restores nothing.
    #[arg(long, value_name = "on|off", value_parser = ["on", "off"])]
    pub keep_image: Option<String>,
    /// Take back a prepared Factory Restore: the next start is the system as usual.
    #[arg(long)]
    pub cancel: bool,
    /// Print what would run, run nothing.
    #[arg(long, short = 'n')]
    pub dry_run: bool,
    /// Skip the typed confirmation (only with --dry-run or a sandbox).
    #[arg(long, short, hide = true)]
    pub yes: bool,
    /// The Recovery window already had ERASE typed: no prompts here (nor the
    /// restart question; the window offers it).
    #[arg(long, hide = true)]
    pub erase_confirmed: bool,
    #[arg(long, hide = true)]
    pub sandbox: Option<PathBuf>,
}

/// The ISO and its signature, downloaded into the user's cache.
fn download(rehearsal: bool) -> Result<(PathBuf, PathBuf)> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .context("no cache directory")?
        .join("bindle/restore");
    let (iso, sig) = (cache.join(ISO), cache.join(format!("{ISO}.sig")));
    let mirror = std::env::var(MIRROR_ENV).unwrap_or_else(|_| MIRROR.to_string());
    if rehearsal {
        println!("would download {mirror}/{ISO} and its signature to {}", cache.display());
        return Ok((iso, sig));
    }
    std::fs::create_dir_all(&cache)?;
    println!("Downloading the Arch Linux installer (about 1.4 GB)…");
    for (url, dest, progress) in [(format!("{mirror}/{ISO}.sig"), &sig, false), (format!("{mirror}/{ISO}"), &iso, true)]
    {
        let mut cmd = Command::new("curl");
        cmd.args(["-fL", "--retry", "3"]);
        if mirror == MIRROR {
            cmd.args(["--proto", "=https", "--tlsv1.2"]);
        }
        cmd.arg("-o").arg(dest).arg(&url);
        cmd.arg(if progress { "--progress-bar" } else { "-sS" });
        if !cmd.status().context("start curl")?.success() {
            bail!("couldn't download {url}");
        }
    }
    Ok((iso, sig))
}

/// Where the ISO comes from: a fresh download when online, else the kept copy.
fn image(rehearsal: bool) -> Result<(String, String)> {
    let kept = Path::new(IMAGE);
    let path = |p: &Path| p.display().to_string();
    if bindle_run::net::problem().is_none() {
        let (iso, sig) = download(rehearsal)?;
        return Ok((path(&iso), path(&sig)));
    }
    if kept.is_file() {
        println!("No internet connection: using the installer kept on this computer.");
        return Ok((path(kept), path(&kept.with_extension("iso.sig"))));
    }
    bail!(
        "Factory Restore needs the internet to download the installer (no copy is kept on this computer). Connect, then try again."
    )
}

pub fn main(a: RestoreArgs) -> Result<()> {
    let launch = Launch::default();
    let log = format!("/var/log/bindle/restore-{}.log", sys::now_id());
    let mut session = Session::new(log, a.dry_run, a.sandbox.clone(), &launch);
    let result = run(&a, &mut session);
    session.quit();
    result
}

fn run(a: &RestoreArgs, session: &mut Session) -> Result<()> {
    let print = &mut |l: &str| println!("{l}");
    if a.cancel {
        session.root(&Request::CancelRestore { keep_image: Path::new(IMAGE).is_file() }, print)?;
        println!("Factory Restore is called off: the next start is this system as usual.");
        return Ok(());
    }
    match a.keep_image.as_deref() {
        Some("on") => {
            let (iso, sig) = download(session.rehearsal())?;
            let (iso, sig) = (iso.display().to_string(), sig.display().to_string());
            session.root(&Request::KeepImage { iso, sig }, print)?;
            println!("A checked copy of the installer is kept on this computer ({IMAGE}).");
            return Ok(());
        }
        Some(_) => {
            session.root(&Request::CancelRestore { keep_image: false }, print)?;
            println!("The installer kept on this computer is deleted.");
            return Ok(());
        }
        None => {}
    }

    println!("Factory Restore erases this computer and installs it from new:");
    println!("  • the computer restarts into the Arch Linux installer, with Bindle");
    println!("  • you choose the disk to erase (this computer's own is picked) and set it up again");
    println!("  • EVERYTHING on that disk goes: your files, apps and settings. Back up what you want to keep first.");
    println!("Nothing is erased until the installer's last question; restarting before then comes back here.");
    let rehearsal = session.rehearsal();
    if !a.erase_confirmed && !(a.yes && rehearsal) && !typed("Type ERASE to continue: ", "ERASE")? {
        bail!("nothing was changed");
    }
    let (iso, sig) = image(rehearsal)?;
    session.root(&Request::PrepareRestore { iso, sig }, print)?;
    if rehearsal || a.erase_confirmed {
        return Ok(());
    }
    if crate::apply::confirm("Restart into the installer now?", true) {
        Command::new("systemctl").arg("reboot").status()?;
    } else {
        println!("Restart when you're ready. To call it off first: bindle restore --cancel");
    }
    Ok(())
}

/// The user typed exactly `word`.
fn typed(prompt: &str, word: &str) -> Result<bool> {
    use std::io::{BufRead, Write};
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim() == word)
}
