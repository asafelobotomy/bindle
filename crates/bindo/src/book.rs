//! Every recipe bindo knows: the shipped ones, then each recipe directory in
//! turn. A recipe with the same name as an earlier one replaces it, so an
//! admin (`/etc/bindle/recipes`) or the user (`~/.config/bindo/recipes`) can
//! change a shipped recipe without editing it.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::recipe::{Origin, Recipe, parse};

/// The recipes built into bindo, by file.
pub const SHIPPED: &[(&str, &str)] = &[
    ("system.toml", include_str!("../recipes/system.toml")),
    ("maintenance.toml", include_str!("../recipes/maintenance.toml")),
    ("setup.toml", include_str!("../recipes/setup.toml")),
    ("toggles.toml", include_str!("../recipes/toggles.toml")),
];

/// Categories in the order the listing shows them; others follow by name.
pub const CATEGORY_ORDER: &[&str] = &["System", "Maintenance", "Setup", "Toggles"];

/// Recipe directories after the shipped set, lowest priority first: what a
/// distribution package adds, what the admin adds, what the user adds.
pub fn default_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/share/bindle/recipes"), PathBuf::from("/etc/bindle/recipes")];
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    if let Some(c) = config {
        dirs.push(c.join("bindo/recipes"));
    }
    dirs
}

/// A directory admin recipes may come from: it and its parents up to `/`
/// are owned by root and not writable by anyone else, so only an admin can
/// change what runs as root after the password prompt.
pub fn trusted(dir: &Path) -> bool {
    dir.ancestors().all(|d| fs::metadata(d).is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0))
}

#[derive(Debug, Default)]
pub struct Book {
    recipes: BTreeMap<String, Recipe>,
    /// Recipe files that could not be read; the rest still load.
    pub warnings: Vec<String>,
}

impl Book {
    /// Only the shipped recipes. Fails on a broken one (the unit tests run this).
    pub fn shipped() -> Result<Self, String> {
        let mut book = Book::default();
        for (file, text) in SHIPPED {
            let recipes = parse(text, &Origin::Shipped).map_err(|e| format!("{file}: {e}"))?;
            for r in recipes {
                if book.recipes.insert(r.name.clone(), r).is_some() {
                    return Err(format!("{file}: a shipped recipe name is used twice"));
                }
            }
        }
        Ok(book)
    }

    /// The shipped recipes, then `*.toml` from each directory in order.
    pub fn load(dirs: &[PathBuf]) -> Self {
        Self::load_with(vec![], dirs)
    }

    /// The shipped recipes, then `modules` (recipes of installed catalog
    /// modules, which ship in the binary too), then the directories.
    pub fn load_with(modules: Vec<Recipe>, dirs: &[PathBuf]) -> Self {
        let mut book = Self::shipped().unwrap_or_else(|e| Book { warnings: vec![e], ..Default::default() });
        book.add(modules);
        for dir in dirs {
            book.add_dir(dir);
        }
        book
    }

    fn add_dir(&mut self, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        let trusted = trusted(dir);
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml") && p.is_file())
            .collect();
        files.sort();
        for path in files {
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| parse(&t, &Origin::File(path.clone())))
            {
                Ok(recipes) => {
                    let (admin, plain): (Vec<_>, Vec<_>) = recipes.into_iter().partition(|r| r.admin && !trusted);
                    for r in admin {
                        self.warnings.push(format!(
                            "skipped admin recipe `{}` in {}: admin recipes go in a root-owned directory such as /etc/bindle/recipes",
                            r.name,
                            path.display()
                        ));
                    }
                    self.add(plain);
                }
                Err(e) => self.warnings.push(format!("skipped {}: {e}", path.display())),
            }
        }
    }

    pub fn add(&mut self, recipes: Vec<Recipe>) {
        for r in recipes {
            self.recipes.insert(r.name.clone(), r);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Recipe> {
        self.recipes.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.recipes.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Recipes grouped by category, categories in [`CATEGORY_ORDER`] first,
    /// recipes by name within each.
    pub fn by_category(&self) -> Vec<(&str, Vec<&Recipe>)> {
        let mut groups: BTreeMap<(usize, &str), Vec<&Recipe>> = BTreeMap::new();
        for r in self.recipes.values() {
            let rank = CATEGORY_ORDER.iter().position(|c| *c == r.category).unwrap_or(CATEGORY_ORDER.len());
            groups.entry((rank, r.category.as_str())).or_default().push(r);
        }
        groups.into_iter().map(|((_, c), rs)| (c, rs)).collect()
    }

    /// Names close to a mistyped one: containing it, or at most two edits away.
    pub fn suggest(&self, typo: &str) -> Vec<&str> {
        self.names().filter(|n| n.contains(typo) || edits(n, typo) <= 2).take(3).collect()
    }
}

/// Levenshtein distance.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = env::temp_dir().join(format!("bindo-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn every_shipped_recipe_is_valid() {
        let book = Book::shipped().unwrap();
        assert!(book.len() >= 12, "only {} shipped recipes", book.len());
        for (_, rs) in book.by_category() {
            for r in rs {
                assert!(CATEGORY_ORDER.contains(&r.category.as_str()), "{}: unknown category", r.name);
                assert!(!r.run.contains("sudo "), "{}: admin recipes use `admin = true`, not sudo", r.name);
            }
        }
    }

    #[test]
    fn categories_keep_their_order() {
        let book = Book::shipped().unwrap();
        let cats: Vec<&str> = book.by_category().iter().map(|(c, _)| *c).collect();
        assert_eq!(cats, CATEGORY_ORDER);
    }

    #[test]
    fn later_directories_override_by_name_and_bad_files_are_skipped() {
        let d = dir("override");
        let text = "[[recipe]]\nname = \"info\"\ncategory = \"Mine\"\ndescription = \"Mine\"\nrun = \"echo hi\"\n";
        fs::write(d.join("a.toml"), text).unwrap();
        fs::write(d.join("b.toml"), "[[recipe]]\nname = 1\n").unwrap();
        fs::write(d.join("notes.txt"), "ignored").unwrap();
        let book = Book::load(&[d.clone(), d.join("missing")]);
        let info = book.get("info").unwrap();
        assert_eq!(info.category, "Mine");
        assert_eq!(info.origin, Origin::File(d.join("a.toml")));
        assert_eq!(book.warnings.len(), 1);
        assert!(book.warnings[0].contains("b.toml"));
        assert!(book.get("clean-journal").is_some(), "the other shipped recipes stay");
    }

    #[test]
    fn admin_recipes_need_a_root_owned_directory() {
        let d = dir("admin");
        let text = "[[recipe]]\nname = \"clean-journal\"\ncategory = \"Maintenance\"\ndescription = \"x\"\nadmin = true\nrun = \"evil\"\n";
        fs::write(d.join("a.toml"), text).unwrap();
        let book = Book::load(std::slice::from_ref(&d));
        assert_eq!(book.get("clean-journal").unwrap().origin, Origin::Shipped);
        assert!(book.warnings[0].contains("root-owned"));
        assert!(!trusted(&d));
        assert!(trusted(Path::new("/usr")));
    }

    #[test]
    fn typos_get_suggestions() {
        let book = Book::shipped().unwrap();
        assert!(book.suggest("clean").iter().all(|n| n.starts_with("clean")));
        assert!(!book.suggest("clean").is_empty());
        assert_eq!(book.suggest("orphan"), ["orphans"]);
        assert_eq!(book.suggest("kernal"), ["kernel"]);
        assert!(book.suggest("zzzzzzz").is_empty());
    }
}
