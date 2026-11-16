//! `bindle undo --list --json` and `--dry-run --json`: the snapshots Bindle
//! took before each apply, and what undoing one would do, for uQ Settings'
//! History tab. Nothing here changes the system.

use std::path::Path;

use anyhow::Result;
use bindle_core::preview::PlanDoc;
use bindle_run::snapshot::{self, Manifest};
use serde_json::{Value, json};

/// `bindle undo`, with its JSON forms (read-only) before the interactive one.
#[allow(clippy::too_many_arguments)]
pub fn cmd(
    id: Option<String>,
    list_all: bool,
    yes: bool,
    force: bool,
    json: bool,
    dry_run: bool,
    sandbox: Option<std::path::PathBuf>,
) -> Result<()> {
    let root = sandbox.clone().unwrap_or_else(|| "/".into());
    match (json, list_all, dry_run) {
        (true, true, _) => println!("{}", serde_json::to_string_pretty(&list(&root))?),
        (true, false, true) => println!("{}", preview(&root, id.as_deref())?.to_json()),
        (true, false, false) => anyhow::bail!("--json goes with --list or --dry-run"),
        (false, _, true) => {
            anyhow::bail!("--dry-run on undo goes with --json (the interactive undo prints its list before it asks)")
        }
        (false, ..) => {
            return crate::apply::undo(
                id,
                list_all,
                force,
                &crate::apply::Options { yes, sandbox, ..Default::default() },
            );
        }
    }
    Ok(())
}

pub fn list(root: &Path) -> Value {
    let rows: Vec<Value> = snapshot::list(root)
        .iter()
        .map(|m| {
            json!({
                "id": m.id, "created": m.created, "label": m.label, "version": m.version,
                "files": m.files.len(), "units": m.units.len(), "packages": m.packages.len() + m.aur.len(),
                "undone": m.undone, "complete": m.complete,
            })
        })
        .collect();
    json!({ "snapshots": rows })
}

/// What undoing `id` (the latest apply and any stopped ones before it when
/// omitted) would do.
pub fn preview(root: &Path, id: Option<&str>) -> Result<PlanDoc> {
    let session = match id {
        Some(id) => vec![snapshot::load(root, id)?],
        None => snapshot::latest_session(&snapshot::list(root)),
    };
    if session.is_empty() {
        let why = if snapshot::list(root).is_empty() {
            "Bindle hasn't changed anything on this system yet."
        } else {
            "Every snapshot has been undone already."
        };
        return Ok(PlanDoc { summary: "Nothing to undo".into(), ..Default::default() }.refuse(why));
    }
    Ok(document(&session))
}

fn document(session: &[Manifest]) -> PlanDoc {
    let mut d = PlanDoc::default();
    for m in session {
        for f in &m.files {
            let (label, rev) = if f.existed { ("Put back", Some(true)) } else { ("Remove", Some(false)) };
            d.change(label, f.path.clone(), "", rev);
        }
        for u in &m.units {
            d.change("Turn off services", u.clone(), "systemctl disable --now", Some(true));
        }
        for p in m.packages.iter().chain(&m.aur) {
            d.change("Stay installed", p.clone(), "the command to remove them is printed at the end", None);
        }
        for c in &m.manual {
            d.change("Not reverted", c.clone(), "undo by hand if needed", Some(false));
        }
    }
    let files: usize = session.iter().map(|m| m.files.len()).sum();
    d.summary = format!("Undo {} apply run(s): {files} file(s)", session.len());
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Manifest {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_snapshot_becomes_grouped_changes() {
        let m = manifest(
            r#"{"id":"1","created":"now","label":"zram","user":"u","files":[{"path":"/etc/a","existed":true,"mode":null,"stored":null},{"path":"/etc/b","existed":false,"mode":null,"stored":null}],"units":["zswap.service"],"packages":["zram-generator"],"aur":[]}"#,
        );
        let d = document(&[m]);
        let groups: Vec<&str> = d.changes.iter().map(|c| c.group.as_str()).collect();
        assert_eq!(groups, ["Put back", "Remove", "Turn off services", "Stay installed"]);
        assert_eq!(d.changes[1].reversible, Some(false));
        assert!(!d.would_refuse);
    }

    #[test]
    fn no_snapshots_is_nothing_to_undo() {
        let dir = std::env::temp_dir().join(format!("bindle-undo-json-{}", std::process::id()));
        let d = preview(&dir, None).unwrap();
        assert!(d.would_refuse);
        assert_eq!(list(&dir)["snapshots"].as_array().unwrap().len(), 0);
    }
}
