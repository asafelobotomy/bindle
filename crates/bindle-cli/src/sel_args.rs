//! The selection flags `plan` and `apply` share.

use std::path::PathBuf;

use anyhow::Result;
use bindle_core::Selection;
use bindle_run::selection;
use clap::Args;

#[derive(Args)]
pub struct SelArgs {
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
    pub fn resolve(self) -> Result<Selection> {
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
