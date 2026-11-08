//! Every source file stays within 400 lines (AGENTS.md convention).

use std::fs;
use std::path::{Path, PathBuf};

const LIMIT: usize = 400;
const SOURCE_EXTENSIONS: [&str; 3] = ["rs", "py", "sh"];
const SKIP_DIRS: [&str; 5] = ["target", ".git", ".work", "dist", "AppDir"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()) {
                walk(&p, out);
            }
        } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| SOURCE_EXTENSIONS.contains(&x)) {
            out.push(p);
        }
    }
}

#[test]
fn no_source_file_exceeds_400_lines() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() > 20, "found only {} source files under {}", files.len(), root.display());
    let too_long: Vec<String> = files
        .iter()
        .filter_map(|f| {
            let lines = fs::read_to_string(f).ok()?.lines().count();
            (lines > LIMIT).then(|| format!("{}: {lines} lines", f.strip_prefix(&root).unwrap().display()))
        })
        .collect();
    assert!(too_long.is_empty(), "split these files (limit {LIMIT}):\n{}", too_long.join("\n"));
}
