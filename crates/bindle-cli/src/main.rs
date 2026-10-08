mod apply;
mod firstlogin;
mod iso;
mod notify;
mod print;
mod selection;
mod terminal;
mod tui;
mod welcome;

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Result, bail};
use bindle_core::{Catalog, Facts, Selection, plan};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bindle", version, about = "Pack your fresh Linux install for the road.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Interactive wizard (default when run in a terminal).
    Wizard {
        #[arg(long, hide = true)]
        sandbox: Option<PathBuf>,
    },
    /// Show detected hardware and system facts.
    Detect {
        #[arg(long)]
        json: bool,
    },
    /// Show what archinstall set up (from its log, or rebuilt from pacman.log).
    Archinstall {
        #[arg(long)]
        json: bool,
    },
    /// List profiles, categories and modules.
    List,
    /// Preview what a selection would change. Never modifies the system.
    Plan {
        #[command(flatten)]
        sel: SelArgs,
        #[arg(long)]
        json: bool,
    },
    /// Apply a selection: snapshot, then run each step.
    Apply {
        #[command(flatten)]
        sel: SelArgs,
        /// Don't ask for confirmation.
        #[arg(long, short)]
        yes: bool,
        /// Run against a scratch directory without root, recording commands only.
        #[arg(long, hide = true)]
        sandbox: Option<PathBuf>,
    },
    /// Restore files from a pre-apply snapshot (latest by default).
    Undo {
        id: Option<String>,
        #[arg(long)]
        list: bool,
        #[arg(long, short)]
        yes: bool,
        #[arg(long, hide = true)]
        sandbox: Option<PathBuf>,
    },
    /// Open Bindle automatically on each user's first login.
    FirstLogin {
        #[command(subcommand)]
        action: FirstLoginCmd,
    },
    /// Install Arch from the live ISO: archinstall with Bindle's presets, and
    /// Bindle opening on first login.
    Iso {
        /// Pre-select a desktop (bindle id, e.g. plasma, gnome, hyprland).
        #[arg(long)]
        desktop: Option<String>,
        /// Bindle profile for first login: desktop, gaming, developer, minimal.
        #[arg(long, short)]
        profile: Option<String>,
        /// Skip the pre-install screen (needs --profile).
        #[arg(long, short)]
        yes: bool,
        /// Write the preset and plugin, print the archinstall command, run nothing.
        #[arg(long)]
        dry_run: bool,
        /// archinstall config merged over Bindle's preset (your values win),
        /// e.g. a saved user_configuration.json with disks and hostname.
        #[arg(long, value_name = "JSON")]
        config: Option<PathBuf>,
        /// archinstall credentials file (users, root password), passed through.
        #[arg(long, value_name = "JSON")]
        creds: Option<PathBuf>,
        /// Unattended install: archinstall asks nothing (needs --config with disks and --creds).
        #[arg(long)]
        silent: bool,
        /// Keep the desktops' own welcome apps (by default Bindle replaces them).
        #[arg(long)]
        keep_welcome: bool,
        #[arg(long, hide = true)]
        workdir: Option<PathBuf>,
    },
    /// Root helper used by `apply` (internal).
    #[command(hide = true)]
    Helper {
        #[arg(long)]
        log: String,
        #[arg(long, default_value = "/")]
        root: PathBuf,
        #[arg(long)]
        fake_commands: bool,
    },
}

#[derive(Subcommand)]
enum FirstLoginCmd {
    /// Install bindle system-wide with autostart (root). `--root /mnt` from the live ISO.
    Install {
        #[arg(long, default_value = "/")]
        root: PathBuf,
        /// Skip the console-login hook (/etc/profile.d).
        #[arg(long)]
        no_tty_hook: bool,
    },
    /// Remove the autostart entries (root); `--purge` also removes the binary.
    Remove {
        #[arg(long, default_value = "/")]
        root: PathBuf,
        #[arg(long)]
        purge: bool,
    },
    /// Show what is installed and whether this user has seen the wizard.
    Status {
        #[arg(long, default_value = "/")]
        root: PathBuf,
    },
    /// Run the first-login wizard (used by the autostart entry).
    Run {
        #[arg(long, hide = true)]
        in_terminal: bool,
    },
    /// Turn off desktop welcome apps (GNOME tour, KDE Welcome Center, Hyprland
    /// welcome) so Bindle is the first-login welcome (root; used by `bindle iso`).
    QuietWelcome {
        #[arg(long, default_value = "/")]
        root: PathBuf,
    },
    /// Stop first-login reminders for this user.
    Dismiss,
    /// Wait for a graphical session, then open the wizard (systemd user service).
    #[command(hide = true)]
    Watch,
    /// Print a custom_commands snippet for an archinstall config file.
    Snippet {
        /// Download URL of the bindle binary.
        #[arg(long)]
        url: String,
    },
}

#[derive(Args)]
struct SelArgs {
    /// Start from a profile: minimal, desktop, gaming, developer.
    #[arg(long, short)]
    profile: Option<String>,
    /// Add a module (repeatable).
    #[arg(long = "with", value_name = "MODULE")]
    include: Vec<String>,
    /// Remove a module (repeatable).
    #[arg(long = "without", value_name = "MODULE")]
    exclude: Vec<String>,
    /// Start from a saved selection file (see ~/.config/bindle/selection.toml).
    #[arg(long, value_name = "FILE")]
    from: Option<PathBuf>,
}

impl SelArgs {
    fn resolve(self) -> Result<Selection> {
        let cli = Selection { profile: self.profile, include: self.include, exclude: self.exclude };
        match self.from {
            Some(path) => Ok(selection::merge(selection::load(&path)?, cli)),
            None => Ok(cli),
        }
    }
}

/// Written into the new system by the archinstall plugin.
const SYSTEM_SELECTION: &str = "/etc/bindle/selection.toml";

fn read_current(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Cmd::Wizard { sandbox: None });
    // Commands that don't need detection (and may run as root).
    match command {
        Cmd::Helper { log, root, fake_commands } => {
            return apply::helper::serve(apply::helper::HelperOpts { root, fake_commands, log });
        }
        Cmd::FirstLogin { action } if !matches!(action, FirstLoginCmd::Run { .. }) => return first_login(action),
        Cmd::Undo { id, list, yes, sandbox } => return apply::undo(id, list, &apply::Options { yes, sandbox }),
        _ => {}
    }

    let catalog = Catalog::arch()?;
    let facts = Facts::detect();
    match command {
        Cmd::Wizard { sandbox } => {
            if !std::io::stdout().is_terminal() {
                bail!("the wizard needs a terminal; try `bindle plan --help`");
            }
            warn_distro(&facts);
            wizard(&catalog, &facts, sandbox).map(drop)
        }
        Cmd::Iso { desktop, profile, yes, dry_run, config, creds, silent, keep_welcome, workdir } => iso::run(
            &facts,
            iso::IsoOpts { desktop, profile, yes, dry_run, config, creds, silent, keep_welcome, workdir },
        ),
        Cmd::FirstLogin { action: FirstLoginCmd::Run { in_terminal } } => {
            first_login_run(&catalog, &facts, in_terminal)
        }
        Cmd::Detect { json } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&facts)?);
            } else {
                print::facts(&facts);
            }
            Ok(())
        }
        Cmd::Archinstall { json } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&facts.install)?);
            } else {
                print::install_lines(&facts).iter().for_each(|l| println!("{l}"));
            }
            Ok(())
        }
        Cmd::List => {
            print::list(&catalog, &facts);
            Ok(())
        }
        Cmd::Plan { sel, json } => {
            warn_distro(&facts);
            let p = plan::resolve(&catalog, &facts, &sel.resolve()?, &read_current)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                print::plan_lines(&p).iter().for_each(|l| println!("{l}"));
            }
            Ok(())
        }
        Cmd::Apply { sel, yes, sandbox } => {
            warn_distro(&facts);
            let sel = sel.resolve()?;
            apply_selection(&catalog, &facts, &sel, apply::Options { yes, sandbox }).map(drop)
        }
        Cmd::Helper { .. } | Cmd::Undo { .. } | Cmd::FirstLogin { .. } => unreachable!("handled above"),
    }
}

/// Resolve, apply, and remember the selection on success.
fn apply_selection(catalog: &Catalog, facts: &Facts, sel: &Selection, opts: apply::Options) -> Result<bool> {
    let current = |p: &str| match &opts.sandbox {
        Some(root) => std::fs::read_to_string(apply::sys::host(root, p)).ok(),
        None => read_current(p),
    };
    let p = plan::resolve(catalog, facts, sel, &current)?;
    let outcome = apply::apply(&p, facts, &opts)?;
    if outcome == apply::Outcome::Applied
        && opts.sandbox.is_none()
        && let Some(path) = selection::default_path()
    {
        selection::save(sel, &path)?;
        println!("Selection saved to {} (replay with `bindle apply --from`).", path.display());
    }
    Ok(outcome != apply::Outcome::Cancelled)
}

/// Run the wizard; if the user chose to apply, apply. Returns whether the
/// user went through with it (applied, or nothing was left to do).
fn wizard(catalog: &Catalog, facts: &Facts, sandbox: Option<PathBuf>) -> Result<bool> {
    // A profile chosen before install (`bindle iso`) seeds the wizard.
    let seed = selection::load(std::path::Path::new(SYSTEM_SELECTION)).ok();
    match tui::run(catalog, facts, seed)? {
        Some(sel) => apply_selection(catalog, facts, &sel, apply::Options { yes: false, sandbox }),
        None => Ok(false),
    }
}

fn first_login(action: FirstLoginCmd) -> Result<()> {
    let lines = match action {
        FirstLoginCmd::Install { root, no_tty_hook } => firstlogin::install(&root, !no_tty_hook)?,
        FirstLoginCmd::Remove { root, purge } => firstlogin::remove(&root, purge)?,
        FirstLoginCmd::Status { root } => firstlogin::status(&root),
        FirstLoginCmd::Snippet { url } => vec![firstlogin::archinstall_snippet(&url)],
        FirstLoginCmd::Watch => return firstlogin::watch(),
        FirstLoginCmd::QuietWelcome { root } => {
            if root == std::path::Path::new("/") && !apply::sys::is_root() {
                bail!("needs root: sudo bindle first-login quiet-welcome");
            }
            welcome::quiet(&root)?
        }
        FirstLoginCmd::Dismiss => {
            firstlogin::mark_done()?;
            vec!["Bindle won't open or remind you at login again. Run `bindle` any time.".into()]
        }
        FirstLoginCmd::Run { .. } => unreachable!("needs detection"),
    };
    lines.iter().for_each(|l| println!("{l}"));
    Ok(())
}

fn first_login_run(catalog: &Catalog, facts: &Facts, in_terminal: bool) -> Result<()> {
    if firstlogin::is_done() || apply::sys::is_root() {
        return Ok(());
    }
    let seen = firstlogin::is_seen();
    if !std::io::stdin().is_terminal() {
        if in_terminal {
            bail!("no terminal attached");
        }
        if !firstlogin::claim_launch() {
            return Ok(()); // already handled in this session (autostart + user service)
        }
        if seen {
            // Later logins get a reminder, not the wizard. Without a notification
            // server, fall back to opening the wizard so the reminder isn't lost.
            match notify::remind() {
                Ok(notify::Choice::Open) | Err(_) => {}
                Ok(notify::Choice::Dismiss) => {
                    firstlogin::release_launch();
                    return firstlogin::mark_done();
                }
                Ok(notify::Choice::Ignored) => {
                    firstlogin::release_launch();
                    return Ok(());
                }
            }
        } else {
            // Open after the desktop's own welcome app, so Bindle ends up on top.
            welcome::wait_for_welcome_apps();
        }
        let exe = std::env::current_exe()?.to_string_lossy().into_owned();
        return terminal::spawn(&[exe, "first-login".into(), "run".into(), "--in-terminal".into()]).inspect_err(|_| {
            firstlogin::release_launch();
        });
    }
    if seen && !in_terminal {
        // Console login after the first: a one-line reminder instead of the wizard.
        println!(
            "Bindle: this system isn't fully set up yet. Run `bindle` to continue, or `bindle first-login dismiss`."
        );
        return Ok(());
    }
    firstlogin::mark_seen()?;
    let finished = wizard(catalog, facts, None)?;
    if finished || !apply::confirm("Remind me at my next login?", true) {
        firstlogin::mark_done()?;
        println!("Bindle won't open or remind you again. Run `bindle` any time.");
    } else {
        println!("Bindle will remind you with a notification at your next login.");
    }
    if in_terminal {
        firstlogin::release_launch();
        apply::confirm("Press Enter to close.", true);
    }
    Ok(())
}

fn warn_distro(facts: &Facts) {
    if !facts.is_arch_family() {
        eprintln!("warning: Bindle currently targets Arch Linux; detected `{}`. Plans may be wrong.", facts.distro_id);
    }
}
