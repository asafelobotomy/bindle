//! `bindle rebase`: the system as installed, again (see
//! bindle_core::recover::rebase). Stops at the first step that fails;
//! nothing after it runs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use bindle_core::Step;
use bindle_core::recover::rebase::{self, Changed};
use bindle_run::client::Launch;
use bindle_run::protocol::{ConfigFile, Request};
use bindle_run::session::Session;
use bindle_run::{selection, sys};

#[derive(clap::Args, Debug)]
pub struct RebaseArgs {
    /// Don't ask for confirmation.
    #[arg(long, short)]
    pub yes: bool,
    /// Print what would run, run nothing.
    #[arg(long, short = 'n')]
    pub dry_run: bool,
    /// Put back the config files an earlier rebase reset (its id is in the report).
    #[arg(long, value_name = "ID")]
    pub undo_config: Option<String>,
    #[arg(long, hide = true)]
    pub sandbox: Option<PathBuf>,
}

/// Every word of a command's output (package names).
fn words(argv: &[&str]) -> BTreeSet<String> {
    let out = Command::new(argv[0]).args(&argv[1..]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().map(String::from).collect()).unwrap_or_default()
}

fn shown(list: &[String]) -> String {
    let mut s = list.iter().take(12).cloned().collect::<Vec<_>>().join(" ");
    if list.len() > 12 {
        s.push_str(&format!(" … and {} more", list.len() - 12));
    }
    s
}

pub fn main(a: RebaseArgs) -> Result<()> {
    let launch = Launch::default();
    let id = sys::now_id();
    let mut session = Session::new(format!("/var/log/bindle/rebase-{id}.log"), a.dry_run, a.sandbox.clone(), &launch);
    let result = run(&a, &id, &mut session);
    session.quit();
    result
}

fn run(a: &RebaseArgs, id: &str, session: &mut Session) -> Result<()> {
    let print = &mut |l: &str| println!("{l}");
    if let Some(old) = &a.undo_config {
        if !bindle_run::snapshot::valid_id(old) {
            bail!("{old:?} isn't a rebase id");
        }
        let dir = format!("/var/lib/bindle/rebase/{old}/etc/.");
        let argv = ["cp", "-a", &dir, "/etc/"].map(String::from).to_vec();
        session.root(&Request::Run { step: Step::RunAsRoot { module: "rebase".into(), argv } }, print)?;
        println!("The config files rebase {old} reset are back as they were.");
        return Ok(());
    }

    let log = std::fs::read_to_string("/var/log/pacman.log").unwrap_or_default();
    let record = bindle_run::whatsnew::record(Path::new("/"));
    let stock = rebase::stock(&log, record.modules.values().flat_map(|c| c.packages.clone()));
    if stock.is_empty() {
        bail!("Bindle can't tell what was installed at first: /var/log/pacman.log has no record of the installer");
    }
    let installed = words(&["pacman", "-Qq"]);
    let sync = words(&["pacman", "-Slq"]);
    let group_names: BTreeSet<String> = {
        let out = Command::new("pacman").arg("-Sg").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
        out.unwrap_or_default().lines().filter_map(|l| l.split_whitespace().next()).map(String::from).collect()
    };
    let mut p = rebase::packages(&stock, &installed, &|n| sync.contains(n) || group_names.contains(n));
    // A group's members are reinstalled, not "put back".
    let (grp, restore): (Vec<String>, Vec<String>) = p.restore.into_iter().partition(|n| group_names.contains(n));
    p.reinstall.extend(grp);
    p.restore = restore;
    let mut qkk_argv = vec!["pacman".to_string(), "-Qkk".into(), "--".into()];
    qkk_argv.extend(p.reinstall.iter().filter(|n| installed.contains(*n)).cloned());
    let qkk = Command::new("pacman").args(&qkk_argv[1..]).output().context("run pacman -Qkk")?;
    let changed = rebase::changed_configs(&String::from_utf8_lossy(&qkk.stdout), &stock);
    let snapper = Path::new("/etc/snapper/configs/root").exists();
    let yours = installed.difference(&stock).count();

    println!("Rebase puts the system back to how it was installed, Bindle's setup included:");
    println!("  • reinstall {} packages (installed at first, or by Bindle)", p.reinstall.len());
    if !p.restore.is_empty() {
        println!("  • put back {} removed: {}", p.restore.len(), shown(&p.restore));
    }
    if changed.is_empty() {
        println!("  • no config file of theirs was changed");
    } else {
        println!("  • reset {} changed config file(s) to the package's version (your copy is kept):", changed.len());
        changed.iter().for_each(|c| println!("      {} ({})", c.path, c.package));
    }
    println!("  • then apply Bindle's setup again");
    println!(
        "Stays as it is: your home folder, the {yours} package(s) you added yourself, Flatpaks, and this machine's own \
         settings (accounts, disks, its name, language, boot)."
    );
    if snapper {
        println!("A snapshot is taken first: `bindle rollback` goes back to it.");
    } else {
        println!("There is no snapper snapshot to go back to on this system.");
    }
    if !a.yes && !crate::apply::confirm("Rebase now?", false) {
        bail!("nothing was changed");
    }

    let root = |argv: Vec<String>| Request::Run { step: Step::RunAsRoot { module: "rebase".into(), argv } };
    let argv =
        |head: &[&str], rest: &[String]| head.iter().map(|s| s.to_string()).chain(rest.iter().cloned()).collect();
    if snapper {
        let snap = argv(&["snapper", "-c", "root", "create", "--cleanup-algorithm", "number", "--description"], &[]);
        session.root(&root([snap, vec!["before bindle rebase".into()]].concat()), print)?;
    }
    if !p.reinstall.is_empty() {
        session.root(&root(argv(&["pacman", "-S", "--noconfirm", "--"], &p.reinstall)), print)?;
    }
    if !p.restore.is_empty() {
        session.root(&root(argv(&["pacman", "-S", "--needed", "--noconfirm", "--"], &p.restore)), print)?;
    }
    let files: Vec<ConfigFile> = changed
        .iter()
        .map(|Changed { package, path }| ConfigFile { package: package.clone(), path: path.clone() })
        .collect();
    session.root(&Request::ResetConfig { id: id.to_string(), files }, print)?;
    session.quit();

    // Bindle's setup again, from the selection it last applied.
    let sel = selection::default_path()
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(crate::apply::SYSTEM_SELECTION));
    if sel.is_file() {
        let exe = std::env::current_exe()?;
        let mut cmd = Command::new(exe);
        cmd.arg("apply").arg("--from").arg(&sel).arg("--yes");
        if let Some(dir) = &a.sandbox {
            cmd.arg("--sandbox").arg(dir);
        }
        if a.dry_run {
            println!("would run: bindle apply --from {} --yes", sel.display());
        } else if !cmd.status()?.success() {
            bail!("Bindle's setup didn't finish; run `bindle apply --from {}` again", sel.display());
        }
    }
    println!("Rebase done.");
    if !changed.is_empty() {
        println!(
            "The config files you had changed are in /var/lib/bindle/rebase/{id}/ (`bindle rebase --undo-config {id}` puts them back)."
        );
    }
    Ok(())
}
