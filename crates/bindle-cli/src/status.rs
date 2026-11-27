//! `bindle status`: the selection Bindle last applied for this user, when, and
//! what of it isn't in place any more. uQ Settings' "Re-run setup" page reads
//! the JSON form (`uq/pages/bindle/`).

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use bindle_core::{Catalog, Facts, Selection, Step, plan};
use bindle_run::selection;
use serde::Serialize;

use crate::print;

#[derive(Debug, Serialize, PartialEq)]
pub struct Status {
    /// The saved selection read, or none when Bindle hasn't applied anything yet.
    pub selection: Option<PathBuf>,
    pub profile: Option<String>,
    /// Modules added to and removed from the profile.
    pub with: Vec<String>,
    pub without: Vec<String>,
    /// When the selection was saved (the last successful apply), in Unix seconds.
    pub last_applied: Option<u64>,
    /// The steps applying the selection again would run, one line each;
    /// empty when everything is in place.
    pub missing: Vec<String>,
    /// The same steps grouped by what they do, for a page that shows a summary first.
    pub groups: Vec<Group>,
    /// The saved selection couldn't be read or planned.
    pub error: Option<String>,
}

/// Missing steps of one kind: `kind` is packages, files, services, commands or other; `label`
/// says what they amount to ("Install 84 packages"); `items` are the names to show under it
/// (packages, or the modules that write files or run commands).
#[derive(Debug, Serialize, PartialEq)]
pub struct Group {
    pub kind: &'static str,
    pub label: String,
    pub items: Vec<String>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Sort a plan's steps into a few kinds a person can read at a glance.
pub fn group_steps(catalog: &Catalog, steps: &[Step]) -> Vec<Group> {
    let module = |id: &str| catalog.module(id).map_or_else(|| id.to_string(), |m| m.name.clone());
    let (mut packages, mut files, mut units, mut commands, mut other) = (vec![], vec![], vec![], vec![], vec![]);
    let mut file_count = 0;
    let mut command_count = 0;
    let mut unit_count = 0;
    let mut other_count = 0;
    let push_once = |v: &mut Vec<String>, s: String| {
        if !v.contains(&s) {
            v.push(s);
        }
    };
    for step in steps {
        match step {
            Step::InstallPackages { packages: p } | Step::InstallAur { packages: p } => {
                packages.extend(p.iter().cloned())
            }
            Step::BootstrapAurHelper { package } => packages.push(package.clone()),
            Step::InstallReleases { assets } => packages.extend(assets.iter().map(|a| a.name.clone())),
            Step::WriteFile { module: m, .. } => {
                file_count += 1;
                push_once(&mut files, module(m));
            }
            Step::EditPacmanConf { .. } | Step::RemoveFiles { .. } | Step::MountAtBoot { .. } => {
                file_count += 1;
                push_once(&mut files, "System settings".into());
            }
            Step::EnableUnits { units: u } | Step::DisableUnits { units: u } => {
                unit_count += u.len();
                units.extend(u.iter().cloned());
            }
            Step::RunAsRoot { module: m, .. } | Step::RunAsUser { module: m, .. } => {
                command_count += 1;
                push_once(&mut commands, module(m));
            }
            Step::RebuildInitramfs => {
                command_count += 1;
                push_once(&mut commands, "Boot image".into());
            }
            Step::RemovePackages { .. } | Step::AddToGroups { .. } => {
                other_count += 1;
                push_once(&mut other, "Replace or add to what was installed".into());
            }
        }
    }
    packages.sort();
    packages.dedup();
    let mut out = vec![];
    if !packages.is_empty() {
        out.push(Group {
            kind: "packages",
            label: format!("Install {}", plural(packages.len(), "package", "packages")),
            items: packages,
        });
    }
    if file_count > 0 {
        out.push(Group {
            kind: "files",
            label: format!("Write {}", plural(file_count, "settings file", "settings files")),
            items: files,
        });
    }
    if unit_count > 0 {
        out.push(Group {
            kind: "services",
            label: format!("Turn on {}", plural(unit_count, "service or timer", "services and timers")),
            items: units,
        });
    }
    if command_count > 0 {
        out.push(Group {
            kind: "commands",
            label: format!("Run {}", plural(command_count, "setup command", "setup commands")),
            items: commands,
        });
    }
    if other_count > 0 {
        out.push(Group { kind: "other", label: plural(other_count, "other change", "other changes"), items: other });
    }
    out
}

/// The user's saved selection, else the one `bindle iso` left for the system.
fn saved(system: &Path) -> Option<PathBuf> {
    selection::default_path().filter(|p| p.is_file()).or_else(|| system.is_file().then(|| system.to_path_buf()))
}

pub fn collect(
    catalog: &Catalog,
    facts: &Facts,
    path: Option<PathBuf>,
    current: &dyn Fn(&str) -> Option<String>,
) -> Status {
    let mut st = Status {
        selection: None,
        profile: None,
        with: vec![],
        without: vec![],
        last_applied: None,
        missing: vec![],
        groups: vec![],
        error: None,
    };
    let Some(path) = path else { return st };
    st.last_applied = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    st.selection = Some(path.clone());
    let sel = match selection::load(&path) {
        Ok(s) => s,
        Err(e) => {
            st.error = Some(format!("{e:#}"));
            return st;
        }
    };
    st.profile.clone_from(&sel.profile);
    st.with.clone_from(&sel.include);
    st.without.clone_from(&sel.exclude);
    // Plan as an interactive apply would, conflicts included.
    let sel = Selection { replace_conflicts: true, ..sel };
    match plan::resolve(catalog, facts, &sel, current) {
        Ok(p) => {
            st.missing = p.steps.iter().map(print::describe).collect();
            st.groups = group_steps(catalog, &p.steps);
        }
        Err(e) => st.error = Some(format!("{e:#}")),
    }
    st
}

pub fn run(catalog: &Catalog, facts: &Facts, json: bool, system: &Path) -> Result<()> {
    let st = collect(catalog, facts, saved(system), &|p| std::fs::read_to_string(p).ok());
    if json {
        println!("{}", serde_json::to_string_pretty(&st)?);
        return Ok(());
    }
    text(&st).iter().for_each(|l| println!("{l}"));
    Ok(())
}

/// The plain-text report: one line per kind of step with what it covers, not each step's
/// command line (a setup command can be a whole shell script). `--json` keeps every step.
fn text(st: &Status) -> Vec<String> {
    let Some(path) = &st.selection else {
        return vec!["Bindle hasn't set up anything here yet. Run `bindle` to start.".into()];
    };
    let mut out =
        vec![format!("Selection: {}", path.display()), format!("Profile: {}", st.profile.as_deref().unwrap_or("none"))];
    if let Some(e) = &st.error {
        out.push(format!("It can't be checked: {e}"));
    } else if st.missing.is_empty() {
        out.push("Everything it set up is still in place.".into());
    } else {
        out.push("Applying it again would:".into());
        for g in &st.groups {
            if g.items.is_empty() {
                out.push(format!("  {}", g.label));
            } else {
                out.push(format!("  {}: {}", g.label, g.items.join(", ")));
            }
        }
        out.push("`bindle status --json` lists every step.".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_saved_is_not_an_error() {
        let catalog = Catalog::arch().unwrap();
        let st = collect(&catalog, &Facts::default(), None, &|_| None);
        assert_eq!(st.selection, None);
        assert!(st.missing.is_empty() && st.error.is_none());
    }

    #[test]
    fn a_saved_selection_lists_what_is_missing() {
        let dir = std::env::temp_dir().join(format!("bindle-status-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("selection.toml");
        std::fs::write(&path, "profile = \"minimal\"\nwith = [\"fstrim\"]\n").unwrap();
        let catalog = Catalog::arch().unwrap();
        let st = collect(&catalog, &Facts::default(), Some(path.clone()), &|_| None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(st.profile.as_deref(), Some("minimal"));
        assert_eq!(st.with, ["fstrim"]);
        assert!(st.last_applied.is_some());
        assert!(st.error.is_none(), "{:?}", st.error);
        assert!(!st.missing.is_empty(), "nothing is installed in a blank plan");
        let kinds: Vec<&str> = st.groups.iter().map(|g| g.kind).collect();
        assert!(kinds.contains(&"packages"), "{kinds:?}");
        assert!(st.groups.iter().all(|g| !g.items.is_empty() && !g.label.is_empty()));
    }

    #[test]
    fn the_text_report_summarises_instead_of_printing_scripts() {
        let st = Status {
            selection: Some(PathBuf::from("/home/me/.config/bindle/selection.toml")),
            profile: Some("desktop".into()),
            with: vec![],
            without: vec![],
            last_applied: None,
            missing: vec![
                "sh -c f=\"$HOME/.bashrc\"\n[ -f \"$f\" ] && exit 0\ncat >>\"$f\" <<'EOF' [bash-extras]".into(),
            ],
            groups: vec![
                Group { kind: "packages", label: "Install 1 package".into(), items: vec!["rtkit".into()] },
                Group {
                    kind: "commands",
                    label: "Run 1 setup command".into(),
                    items: vec!["A friendlier terminal shell".into()],
                },
            ],
            error: None,
        };
        let out = text(&st);
        assert_eq!(
            out[2..],
            [
                "Applying it again would:",
                "  Install 1 package: rtkit",
                "  Run 1 setup command: A friendlier terminal shell",
                "`bindle status --json` lists every step.",
            ]
        );
        assert!(out.iter().all(|l| !l.contains("cat >>") && !l.contains('\n')), "{out:?}");
        let done = Status { missing: vec![], groups: vec![], ..st };
        assert_eq!(text(&done).last().unwrap(), "Everything it set up is still in place.");
    }

    #[test]
    fn a_broken_selection_is_reported() {
        let dir = std::env::temp_dir().join(format!("bindle-status-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("selection.toml");
        std::fs::write(&path, "profile = \"no-such-profile\"\n").unwrap();
        let st = collect(&Catalog::arch().unwrap(), &Facts::default(), Some(path), &|_| None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(st.error.as_deref().is_some_and(|e| e.contains("no-such-profile")));
    }
}
