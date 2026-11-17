//! A selection saved before a module was retired still resolves.

use super::*;

fn facts() -> Facts {
    Facts { distro_id: "arch".into(), cpu_vendor: "amd".into(), user: "tester".into(), ..Default::default() }
}

fn plan(sel: Selection) -> Result<Plan> {
    resolve(&Catalog::arch().unwrap(), &facts(), &sel, &|_| None)
}

#[test]
fn a_module_retired_since_the_selection_was_saved_is_skipped_not_an_error() {
    // pdfcraft left the catalog in 0.1.11 (changes.toml `retired`): a selection saved before still lists it.
    let sel = Selection { include: vec!["pdfcraft".into(), "earlyoom".into()], ..Default::default() };
    let p = plan(sel).expect("a retired id is skipped");
    assert!(
        p.steps
            .iter()
            .any(|s| matches!(s, Step::InstallPackages { packages } if packages.iter().any(|x| x == "earlyoom")))
    );
    let nope = Selection { include: vec!["never-existed".into()], ..Default::default() };
    assert!(plan(nope).unwrap_err().to_string().contains("unknown module `never-existed`"));
}
