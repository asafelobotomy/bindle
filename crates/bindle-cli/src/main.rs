mod apply;
mod archinstall_version;
mod carry_network;
mod firstlogin;
mod firstlogin_cmd;
mod gui;
mod icons;
mod install_target;
mod iso;
mod notify;
mod polkit_agent;
mod print;

mod session;
mod status;
mod terminal;
mod tui;
mod uq_handoff;
mod welcome;

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Result, bail};
use bindle_core::{Catalog, Facts, Selection, plan};
use bindle_run::selection;
use clap::{Args, Parser, Subcommand};
use firstlogin_cmd::FirstLoginCmd;

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
    /// What Bindle last set up here, and what of it is missing now.
    Status {
        /// Print it as JSON (uQ Settings' "Re-run setup" page reads this).
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
        /// Save user commands that fail here for first login (install from the ISO).
        #[arg(long, hide = true, value_name = "FILE")]
        defer_user: Option<PathBuf>,
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
        /// Apply the profile at first login instead of before the first reboot.
        #[arg(long)]
        setup_at_first_login: bool,
        #[arg(long, hide = true)]
        workdir: Option<PathBuf>,
    },
    /// The desktop window if it can start, else the wizard in a terminal (the
    /// app-menu entry).
    #[command(hide = true)]
    Open {
        /// Start it in its own session and return at once (uQ Settings'
        /// "Re-run setup" page, which ends its commands' process groups).
        #[arg(long)]
        detach: bool,
    },
    /// Open a terminal running `bindle <args>`, kept open at the end
    /// (the desktop window's fallback when it can't ask for a password).
    #[command(hide = true)]
    InTerminal {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        args: Vec<String>,
    },
    /// Save the live ISO's Wi-Fi (iwd) as NetworkManager connections in the
    /// installed system (run by the archinstall plugin).
    #[command(hide = true)]
    CarryNetwork {
        /// The mounted new system.
        #[arg(long)]
        root: PathBuf,
        /// Where iwd keeps its networks.
        #[arg(long, default_value = carry_network::IWD_DIR)]
        from: PathBuf,
    },
    /// From the live ISO: apply the chosen profile inside the new system
    /// before its first boot (run by the archinstall plugin).
    #[command(hide = true)]
    InstallTarget {
        #[arg(long)]
        root: PathBuf,
        /// The new system's user (default: its first regular user).
        #[arg(long)]
        user: Option<String>,
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

pub(crate) use apply::wizard;
use apply::{SYSTEM_SELECTION, apply_selection, read_current};

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
        Cmd::CarryNetwork { root, from } => return carry_network::run(&from, &root),
        Cmd::InstallTarget { root, user } => return install_target::run(&root, user.as_deref()),
        Cmd::Open { detach: false } => return gui::open(),
        Cmd::Open { detach: true } => return gui::open_detached(),
        Cmd::InTerminal { args } => {
            let exe = std::env::current_exe()?.to_string_lossy().into_owned();
            // Arguments travel as "$@", never through the script text.
            // On failure: try again, or a shell to fix the cause (a fresh
            // desktop may have no other terminal), or close.
            let script = r#"until "$0" "$@"; do
  printf '\nEnter: try again   s: open a shell (exit comes back)   q: close  > '
  read -r a || exit 1
  case "$a" in q|Q) exit 1 ;; s|S) "${SHELL:-/bin/bash}" ;; esac
done
echo; printf 'Press Enter to close. '; read -r _"#;
            let mut argv = vec!["sh".to_string(), "-c".into(), script.into(), exe];
            argv.extend(args);
            return terminal::spawn(&argv);
        }
        Cmd::FirstLogin { action } if !matches!(action, FirstLoginCmd::Run { .. }) => {
            return firstlogin_cmd::dispatch(action);
        }
        Cmd::Undo { id, list, yes, sandbox } => {
            return apply::undo(id, list, &apply::Options { yes, sandbox, ..Default::default() });
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
        Cmd::Iso {
            desktop,
            profile,
            yes,
            dry_run,
            config,
            creds,
            silent,
            keep_welcome,
            setup_at_first_login,
            workdir,
        } => iso::run(
            &catalog,
            &facts,
            iso::IsoOpts {
                desktop,
                profile,
                yes,
                dry_run,
                config,
                creds,
                silent,
                keep_welcome,
                setup_at_first_login,
                workdir,
            },
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
        Cmd::Status { json } => status::run(&catalog, &facts, json, std::path::Path::new(SYSTEM_SELECTION)),
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
        Cmd::Apply { sel, yes, sandbox, defer_user } => {
            warn_distro(&facts);
            let mut sel = sel.resolve()?;
            // Interactive: the confirmation shows what gets replaced.
            sel.replace_conflicts |= !yes;
            apply_selection(&catalog, &facts, &sel, apply::Options { yes, sandbox, defer_user, ..Default::default() })
                .map(drop)
        }
        Cmd::Helper { .. }
        | Cmd::CarryNetwork { .. }
        | Cmd::InstallTarget { .. }
        | Cmd::Open { .. }
        | Cmd::InTerminal { .. }
        | Cmd::Undo { .. }
        | Cmd::FirstLogin { .. } => {
            unreachable!("handled above")
        }
    }
}

fn warn_distro(facts: &Facts) {
    if !facts.is_arch_family() {
        eprintln!("warning: Bindle currently targets Arch Linux; detected `{}`. Plans may be wrong.", facts.distro_id);
    }
}
