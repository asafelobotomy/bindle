mod apply;
mod firstlogin;
mod firstlogin_cmd;
mod gui;
mod iso;
mod notify;
mod print;

mod session;
mod terminal;
mod tui;
mod welcome;

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Result, bail};
use bindle_core::{Catalog, Facts, Selection, plan};
use bindle_run::selection;
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
        /// Print the facts as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show what archinstall set up (from its log, or rebuilt from pacman.log).
    Archinstall {
        /// Print the recovered install as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List profiles, categories and modules.
    List,
    /// Preview what a selection would change. Never modifies the system.
    Plan {
        #[command(flatten)]
        sel: SelArgs,
        /// Print the resolved plan as JSON.
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
        /// Snapshot to restore (see --list); the latest when omitted.
        id: Option<String>,
        /// List snapshots instead of restoring.
        #[arg(long)]
        list: bool,
        /// Don't ask for confirmation.
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
    /// The desktop window if it can start, else the wizard in a terminal (the
    /// app-menu entry).
    #[command(hide = true)]
    Open,
    /// Open a terminal running `bindle <args>`, kept open at the end
    /// (the desktop window's fallback when it can't ask for a password).
    #[command(hide = true)]
    InTerminal {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        args: Vec<String>,
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
        /// Also remove the binary, launcher and icon.
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
        /// Launch-lock token to release (passed from the launching process).
        #[arg(long, hide = true)]
        lock: Option<String>,
    },
    /// Turn off desktop welcome apps (GNOME tour, KDE Welcome Center, Hyprland
    /// welcome) so Bindle is the first-login welcome. As root: system-wide (and
    /// users' homes with --root, used by `bindle iso`); as a user: your own
    /// Hyprland config.
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
        /// Expected SHA-256 of the binary; the command verifies it before running.
        #[arg(long, value_name = "HEX")]
        sha256: Option<String>,
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
    /// Remove installed packages a chosen module conflicts with (e.g. rust for
    /// rustup). Interactive runs do this after you confirm; with --yes,
    /// conflicting modules are skipped unless this is given.
    #[arg(long)]
    replace_conflicts: bool,
}

impl SelArgs {
    fn resolve(self) -> Result<Selection> {
        let cli =
            Selection { profile: self.profile, include: self.include, exclude: self.exclude, ..Default::default() };
        let mut sel = match self.from {
            Some(path) => selection::merge(selection::load(&path)?, cli),
            None => cli,
        };
        sel.replace_conflicts = self.replace_conflicts;
        Ok(sel)
    }
}

/// Written into the new system by the archinstall plugin.
const SYSTEM_SELECTION: &str = "/etc/bindle/selection.toml";

fn read_current(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// `bindle list | head` closes stdout early; `println!` then panics (and the
/// release build aborts). Treat that like SIGPIPE would: quietly stop.
fn quiet_broken_pipe() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or_default();
        if msg.starts_with("failed printing to std") && msg.contains("Broken pipe") {
            std::process::exit(0);
        }
        default(info);
    }));
}

fn main() -> Result<()> {
    quiet_broken_pipe();
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Cmd::Wizard { sandbox: None });
    // Commands that don't need detection (and may run as root).
    match command {
        Cmd::Helper { log, root, fake_commands } => {
            return apply::helper::serve(apply::helper::HelperOpts { root, fake_commands, log });
        }
        Cmd::Open => return gui::open(),
        Cmd::InTerminal { args } => {
            let exe = std::env::current_exe()?.to_string_lossy().into_owned();
            // Arguments travel as "$@", never through the script text.
            let script = r#""$0" "$@"; echo; printf 'Press Enter to close. '; read -r _"#;
            let mut argv = vec!["sh".to_string(), "-c".into(), script.into(), exe];
            argv.extend(args);
            return terminal::spawn(&argv);
        }
        Cmd::FirstLogin { action } if !matches!(action, FirstLoginCmd::Run { .. }) => {
            return firstlogin_cmd::dispatch(action);
        }
        Cmd::Undo { id, list, yes, sandbox } => {
            return apply::undo(id, list, &apply::Options { yes, sandbox, launch: Default::default() });
        }
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
            &catalog,
            &facts,
            iso::IsoOpts { desktop, profile, yes, dry_run, config, creds, silent, keep_welcome, workdir },
        ),
        Cmd::FirstLogin { action: FirstLoginCmd::Run { in_terminal, lock } } => {
            firstlogin_cmd::run(&catalog, &facts, in_terminal, lock)
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
            // Preview what an interactive apply would do.
            let sel = Selection { replace_conflicts: true, ..sel.resolve()? };
            let p = plan::resolve(&catalog, &facts, &sel, &read_current)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                print::plan_lines(&p).iter().for_each(|l| println!("{l}"));
            }
            Ok(())
        }
        Cmd::Apply { sel, yes, sandbox } => {
            warn_distro(&facts);
            let mut sel = sel.resolve()?;
            // Interactive: the confirmation shows what gets replaced.
            sel.replace_conflicts |= !yes;
            apply_selection(&catalog, &facts, &sel, apply::Options { yes, sandbox, launch: Default::default() })
                .map(drop)
        }
        Cmd::Helper { .. } | Cmd::Open | Cmd::InTerminal { .. } | Cmd::Undo { .. } | Cmd::FirstLogin { .. } => {
            unreachable!("handled above")
        }
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
        Some(sel) => {
            apply_selection(catalog, facts, &sel, apply::Options { yes: false, sandbox, launch: Default::default() })
        }
        None => Ok(false),
    }
}

fn warn_distro(facts: &Facts) {
    if !facts.is_arch_family() {
        eprintln!("warning: Bindle currently targets Arch Linux; detected `{}`. Plans may be wrong.", facts.distro_id);
    }
}
