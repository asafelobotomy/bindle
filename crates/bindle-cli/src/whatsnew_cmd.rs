//! `bindle whats-new`: what this Bindle would change compared with what
//! earlier versions set up here, picked in the terminal, then applied
//! through the usual review (snapshot first, so `bindle undo` reverts it).
//! Also opened by itself the first time a newer Bindle runs.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::Result;
use bindle_core::whatsnew::{Kind, WhatsNew};
use bindle_core::{Catalog, Facts};
use bindle_run::whatsnew::Picks;
use bindle_run::{selection, whatsnew, whatsnew_state};
use clap::Args;

use crate::apply::{self, Options, Outcome};
use crate::tui;

#[derive(Args, Debug, Default)]
pub struct WhatsNewArgs {
    /// Print what's new as JSON; change nothing.
    #[arg(long)]
    json: bool,
    /// Apply the ticked items (new and updated modules) without asking.
    #[arg(long, short)]
    yes: bool,
    /// Only list what's new; change nothing.
    #[arg(long)]
    list: bool,
    /// Also show the items you hid.
    #[arg(long)]
    all: bool,
    /// When there is something new, show a desktop notification whose
    /// Review button opens What's new (after Bindle updated itself).
    #[arg(long, conflicts_with_all = ["json", "yes", "list"])]
    notify: bool,
    #[arg(long, hide = true)]
    sandbox: Option<PathBuf>,
}

/// The report as lines (no terminal, `--list`).
pub fn lines(wn: &WhatsNew) -> Vec<String> {
    let since = wn.since.as_deref().map_or("the earlier setup".to_string(), |v| format!("Bindle {v}"));
    let mut out = vec![format!("What's new in Bindle {} since {since}", wn.version)];
    if !wn.recorded {
        out.push("(no record from an older Bindle: compared with what is installed)".into());
    }
    for (kind, title) in tui::whatsnew::GROUPS {
        let items: Vec<_> = wn.items.iter().filter(|i| i.kind == kind).collect();
        if items.is_empty() {
            continue;
        }
        out.push(String::new());
        out.push(format!("{title}:"));
        for i in items {
            out.push(format!("  [{}] {} ({}): {}", if i.ticked { "x" } else { " " }, i.name, i.id, i.why));
            for c in &i.changes {
                out.push(format!("      {}", c.text));
                out.extend(c.diff.iter().map(|d| format!("        {d}")));
            }
        }
    }
    out
}

pub fn run(catalog: &Catalog, facts: &Facts, a: WhatsNewArgs) -> Result<()> {
    let root = a.sandbox.clone().unwrap_or_else(|| PathBuf::from("/"));
    let Some((wn, full)) = whatsnew::look(catalog, facts, &root, a.all)? else {
        println!("Bindle hasn't set anything up here yet: run `bindle` to start.");
        return Ok(());
    };
    if a.json {
        println!("{}", serde_json::to_string_pretty(&wn)?);
        return Ok(());
    }
    if a.notify {
        if !wn.is_empty() {
            let body = format!("{} to review: new features, changes and clean-ups.", wn.items.len());
            let title = format!("What's new in Bindle {}", wn.version);
            if crate::notify::show(&title, &body, &[("review", "Review"), ("later", "Later")])?.as_deref()
                == Some("review")
            {
                crate::gui::whats_new()?;
            }
        }
        return Ok(());
    }
    if wn.is_empty() {
        println!("Nothing new: this system has everything Bindle {} would set up.", wn.version);
        return Ok(());
    }
    let interactive = !a.yes && !a.list && std::io::stdout().is_terminal();
    if !interactive {
        lines(&wn).iter().for_each(|l| println!("{l}"));
    }
    if a.list || (!a.yes && !interactive) {
        return Ok(());
    }
    let defaults = whatsnew::default_picks(&wn);
    let picked = if interactive {
        match tui::whatsnew::run(&wn, defaults)? {
            tui::whatsnew::Ended::Apply(p) => p,
            tui::whatsnew::Ended::Later => {
                println!("Not now: What's new opens again next time (or run `bindle whats-new`).");
                return Ok(());
            }
            tui::whatsnew::Ended::Skip => {
                whatsnew_state::skip_this_version()?;
                println!("Skipped: it opens by itself again for a newer Bindle (`bindle whats-new` any time).");
                return Ok(());
            }
        }
    } else {
        defaults
    };
    apply_picks(
        catalog,
        facts,
        &root,
        &wn,
        &full,
        &picked,
        Options { yes: a.yes, sandbox: a.sandbox, ..Default::default() },
    )
}

/// Review and apply exactly the picks; then the saved selection follows them.
fn apply_picks(
    catalog: &Catalog,
    facts: &Facts,
    root: &Path,
    wn: &WhatsNew,
    full: &bindle_core::Plan,
    picks: &Picks,
    opts: Options,
) -> Result<()> {
    let picked = &picks.items;
    if picked.is_empty() {
        println!("Nothing picked; nothing changed.");
        return Ok(());
    }
    let plan = whatsnew::plan_choice(catalog, facts, root, wn, full, picks)?;
    let kept =
        wn.items.iter().filter(|i| matches!(i.kind, Kind::Retired | Kind::Removed) && picked.contains(&i.id)).count();
    let real = opts.sandbox.is_none();
    let outcome = apply::apply(&plan, facts, &opts)?;
    if matches!(outcome, Outcome::Applied)
        && real
        && let (Some((sel, _)), Some(path)) = (whatsnew::saved(), selection::default_path())
    {
        selection::save(&whatsnew::selection_after(&sel, wn, picks), &path)?;
    }
    if matches!(outcome, Outcome::Applied | Outcome::NothingToDo) && real {
        // Done with this version: it opens by itself again for a newer Bindle.
        whatsnew_state::skip_this_version()?;
    }
    if kept > 0 && matches!(outcome, Outcome::Applied) {
        println!("Cleaned up {kept} retired module(s); their packages stay installed (see above to remove them).");
    }
    Ok(())
}

/// Before the wizard: the first run of a newer Bindle opens What's new when
/// there is something in it. Never fails the wizard.
pub fn first_run(catalog: &Catalog, facts: &Facts, sandbox: Option<PathBuf>) {
    if !std::io::stdout().is_terminal() {
        return;
    }
    let root = sandbox.clone().unwrap_or_else(|| PathBuf::from("/"));
    if let Ok(Some((wn, _))) = whatsnew::look(catalog, facts, &root, false)
        && whatsnew_state::load().should_open(&wn, whatsnew::VERSION)
    {
        let args = WhatsNewArgs { sandbox, ..Default::default() };
        if let Err(e) = run(catalog, facts, args) {
            eprintln!("What's new: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bindle_core::whatsnew::{Change, Item};

    #[test]
    fn the_report_lists_groups_changes_and_diffs() {
        let wn = WhatsNew {
            version: "0.1.9".into(),
            since: Some("0.1.8".into()),
            recorded: true,
            items: vec![Item {
                id: "zram".into(),
                name: "zram swap".into(),
                kind: Kind::Updated,
                why: "changed since it was set up".into(),
                ticked: true,
                changes: vec![Change { diff: vec!["- a".into(), "+ b".into()], ..Change::note("~ changes /etc/z") }],
            }],
        };
        let l = lines(&wn);
        assert_eq!(l[0], "What's new in Bindle 0.1.9 since Bindle 0.1.8");
        assert!(l.contains(&"Changed since it was set up:".to_string()));
        assert!(l.contains(&"  [x] zram swap (zram): changed since it was set up".to_string()));
        assert!(l.contains(&"        - a".to_string()));
    }
}
