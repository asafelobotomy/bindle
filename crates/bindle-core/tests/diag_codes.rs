//! Every `[BINDLE-NNN]` code in the sources is listed in `uq/diag/bindle.toml` (what uQ shows
//! for it), and every listed code is still logged somewhere or marked retired.

use std::collections::BTreeSet;
use std::path::Path;

fn sources(dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "target") {
                sources(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs" || x == "qml" || x == "js")
            && p.file_name().is_some_and(|n| n != "diag_codes.rs" && n != "uq_diag.rs")
        {
            out.push(std::fs::read_to_string(&p).unwrap());
        }
    }
}

fn used(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("[BINDLE-") {
        let code = &rest[i + 1..];
        if code.find(']') == Some(10) {
            out.insert(code[..10].to_string());
        }
        rest = code;
    }
    out
}

#[test]
fn codes_and_the_list_agree() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut texts = Vec::new();
    sources(&root.join("crates"), &mut texts);
    let in_code: BTreeSet<String> = texts.iter().flat_map(|t| used(t)).collect();

    let list: toml::Value =
        toml::from_str(&std::fs::read_to_string(root.join("uq/diag/bindle.toml")).unwrap()).unwrap();
    assert_eq!(list["app"].as_str(), Some("Bindle"));
    let mut listed = BTreeSet::new();
    let mut live = BTreeSet::new();
    for c in list["code"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap().to_string();
        assert!(["error", "warn", "info"].contains(&c["severity"].as_str().unwrap()), "{id}: severity");
        for k in ["title", "means", "fix"] {
            assert!(c[k].as_str().is_some_and(|s| !s.is_empty()), "{id}: no {k}");
        }
        assert!(listed.insert(id.clone()), "{id} is listed twice");
        if !c.get("retired").and_then(|r| r.as_bool()).unwrap_or(false) {
            live.insert(id);
        }
    }
    let unknown: Vec<_> = in_code.difference(&listed).collect();
    assert!(unknown.is_empty(), "logged but not in uq/diag/bindle.toml: {unknown:?}");
    let unused: Vec<_> = live.difference(&in_code).collect();
    assert!(unused.is_empty(), "listed but nothing logs them (retire them): {unused:?}");
}
