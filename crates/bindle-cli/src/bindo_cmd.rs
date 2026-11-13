//! `bindo` (also `bindle bindo`): list and run recipes. The same binary
//! answers to the name `bindo` through a symlink (see `invoked_as`).

use std::ffi::OsString;

use anyhow::{Result, bail};
use bindle_core::catalog::{AssetKind, Module, ReleaseAsset};
use bindo::{Book, args, book, exec, show};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "bindo",
    version,
    about = "Run Bindle's recipes: short, named system tasks.",
    after_help = "Run `bindo` alone to list the recipes, `bindo <recipe> --help` for one recipe's details."
)]
struct BindoCli {
    /// Don't ask before running a recipe that asks first.
    #[arg(long, short)]
    yes: bool,
    /// Print the command a recipe would run, run nothing.
    #[arg(long, short = 'n')]
    dry_run: bool,
    /// List the recipes (also the default with no recipe).
    #[arg(long, short)]
    list: bool,
    /// Print a completion script: bash, zsh or fish.
    #[arg(long, value_name = "SHELL")]
    completions: Option<String>,
    /// Recipe names, one per line (for completion scripts).
    #[arg(long, hide = true)]
    names: bool,
    /// Choices for argument N of a recipe (for completion scripts).
    #[arg(long, hide = true, num_args = 2, value_names = ["RECIPE", "N"])]
    choices: Option<Vec<String>>,
    /// The recipe and its arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "RECIPE [ARGS]")]
    rest: Vec<String>,
}

/// bindo's options may also come after the recipe (`bindo orphans --yes`):
/// recipe arguments never start with `-`, so these can't be mistaken.
struct Split {
    values: Vec<String>,
    yes: bool,
    dry_run: bool,
    help: bool,
}

fn split(given: &[String]) -> Result<Split> {
    let mut s = Split { values: Vec::new(), yes: false, dry_run: false, help: false };
    for a in given {
        match a.as_str() {
            "-y" | "--yes" => s.yes = true,
            "-n" | "--dry-run" => s.dry_run = true,
            "-h" | "--help" => s.help = true,
            o if o.starts_with('-') => bail!("unknown option `{o}`; recipe arguments never start with `-`"),
            v => s.values.push(v.to_string()),
        }
    }
    Ok(s)
}

pub fn main(argv: impl IntoIterator<Item = OsString>) -> Result<()> {
    let cli = BindoCli::parse_from(std::iter::once(OsString::from("bindo")).chain(argv));
    if let Some(sh) = &cli.completions {
        match show::completions(sh) {
            Some(script) => print!("{script}"),
            None => bail!("no completions for `{sh}`; try bash, zsh or fish"),
        }
        return Ok(());
    }
    let book = Book::load_with(module_recipes(), &book::default_dirs());
    for w in &book.warnings {
        eprintln!("bindo: {w}");
    }
    if cli.names {
        book.names().for_each(|n| println!("{n}"));
        return Ok(());
    }
    if let Some(c) = &cli.choices {
        let n: usize = c[1].parse().unwrap_or(usize::MAX);
        if let Some(r) = book.get(&c[0]) {
            args::choices(r, n).iter().for_each(|v| println!("{v}"));
        }
        return Ok(());
    }
    let Some((name, given)) = cli.rest.split_first() else {
        show::list(&book, &exec::on_path).iter().for_each(|l| println!("{l}"));
        return Ok(());
    };
    if cli.list {
        bail!("--list takes no recipe");
    }
    let Some(recipe) = book.get(name) else {
        let near = book.suggest(name);
        let hint = if near.is_empty() { String::new() } else { format!(" Did you mean: {}?", near.join(", ")) };
        bail!("no recipe `{name}`.{hint} Run `bindo` to list them.");
    };
    let opts = split(given)?;
    if opts.help {
        show::help(recipe, &exec::on_path).iter().for_each(|l| println!("{l}"));
        return Ok(());
    }
    let values = args::bind(recipe, &opts.values).map_err(|e| anyhow::anyhow!("{e}. See `bindo {name} --help`."))?;
    let gone = exec::missing(recipe, &exec::on_path);
    if !gone.is_empty() && !(cli.dry_run || opts.dry_run) {
        bail!("`{name}` needs {}, which is not installed", gone.join(", "));
    }
    let dry_run = cli.dry_run || opts.dry_run;
    if let Some(q) = &recipe.confirm
        && !(cli.yes || opts.yes || dry_run)
        && !crate::apply::confirm(q, false)
    {
        return Ok(());
    }
    let how = exec::elevation(recipe, exec::is_root(), &exec::on_path)?;
    if how != exec::Elevate::No && !dry_run {
        eprintln!("bindo: `{name}` runs as root; asking for your password.");
    }
    let code = exec::run(&exec::invocation(recipe, &values, how), dry_run)?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

/// Recipes of the catalog modules installed here, labelled with the module.
fn module_recipes() -> Vec<bindo::Recipe> {
    let Ok(catalog) = bindle_core::Catalog::arch() else { return vec![] };
    let installed: std::collections::BTreeSet<String> = std::process::Command::new("pacman")
        .arg("-Qq")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let has = |name: &str| installed.contains(name);
    let has_appimage =
        |a: &ReleaseAsset| home.as_ref().is_some_and(|h| h.join(a.desktop_entry().trim_start_matches("~/")).is_file());
    catalog
        .modules
        .iter()
        .filter(|m| !m.recipes.is_empty() && module_installed(m, &has, &has_appimage))
        .flat_map(|m| {
            m.recipes.iter().cloned().map(|mut r| {
                r.origin = bindo::Origin::Module(m.name.clone());
                r
            })
        })
        .collect()
}

/// Everything the module installs is there: its packages, AUR packages and
/// pinned releases. A module that installs nothing never counts.
fn module_installed(m: &Module, has: &dyn Fn(&str) -> bool, has_appimage: &dyn Fn(&ReleaseAsset) -> bool) -> bool {
    let names = m.packages.iter().chain(&m.aur);
    let any = !m.packages.is_empty() || !m.aur.is_empty() || !m.releases.is_empty();
    any && names.into_iter().all(|n| has(n))
        && m.releases.iter().all(|r| match r.kind() {
            Some(AssetKind::Package) => has(&r.name),
            _ => has_appimage(r),
        })
}

/// `bindo` and `bindleupgrade`: links to `bindle`, installed beside it.
pub const NAMES: [&str; 2] = ["/usr/local/bin/bindo", "/usr/local/bin/bindleupgrade"];

/// Link [`NAMES`] to `bindle` under `root` (by `bindle first-login install`).
pub fn link_names(root: &std::path::Path) -> Result<String> {
    for name in NAMES {
        let link = bindle_run::sys::host(root, name);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink("bindle", &link).map_err(|e| anyhow::anyhow!("link {name}: {e}"))?;
    }
    Ok(format!("linked {} to bindle", NAMES.join(" and ")))
}

/// One binary, three names: run as `bindo`, do that here (None); as
/// `bindleupgrade`, the arguments of `bindle upgrade`; else `bindle`'s own.
pub fn route() -> Result<Option<Vec<OsString>>> {
    let mut argv: Vec<OsString> = std::env::args_os().collect();
    let appimage_argv0 = std::env::var_os("APPIMAGE").and(std::env::var_os("ARGV0"));
    match invoked_as(argv.first().map(|a| a.as_os_str()), appimage_argv0.as_deref()) {
        Some("bindo") => main(argv.into_iter().skip(1)).map(|()| None),
        Some(_) => {
            argv.insert(1, "upgrade".into());
            Ok(Some(argv))
        }
        None => Ok(Some(argv)),
    }
}

/// The command name this binary was started as: `bindo` or `bindleupgrade`
/// through a symlink, or inside an AppImage (whose runtime sets `ARGV0`).
pub fn invoked_as(argv0: Option<&std::ffi::OsStr>, appimage_argv0: Option<&std::ffi::OsStr>) -> Option<&'static str> {
    let base = |p: &std::ffi::OsStr| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned());
    let names = [appimage_argv0.and_then(base), argv0.and_then(base)];
    names.into_iter().flatten().find_map(|n| match n.as_str() {
        "bindo" => Some("bindo"),
        "bindleupgrade" => Some("bindleupgrade"),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn options_after_the_recipe_are_bindos() {
        let o = split(&s(&["3", "--yes", "-n"])).unwrap();
        assert_eq!(o.values, ["3"]);
        assert!(o.yes && o.dry_run && !o.help);
        assert!(split(&s(&["--root=/"])).is_err());
        assert!(split(&s(&["-h"])).unwrap().help);
    }

    #[test]
    fn the_binary_answers_to_its_other_names() {
        assert_eq!(invoked_as(Some(OsStr::new("/usr/local/bin/bindo")), None), Some("bindo"));
        assert_eq!(invoked_as(Some(OsStr::new("bindleupgrade")), None), Some("bindleupgrade"));
        assert_eq!(invoked_as(Some(OsStr::new("/usr/local/bin/bindle")), None), None);
        let appimage = Some(OsStr::new("/home/u/bin/bindo"));
        assert_eq!(invoked_as(Some(OsStr::new("/tmp/.mount_x/usr/bin/bindle")), appimage), Some("bindo"));
    }

    #[test]
    fn the_names_link_to_bindle() {
        let root = bindle_run::sys::test_dir("bindo-names");
        std::fs::create_dir_all(root.join("usr/local/bin")).unwrap();
        assert!(link_names(&root).unwrap().contains("bindo and"));
        link_names(&root).unwrap();
        for n in NAMES {
            assert_eq!(std::fs::read_link(bindle_run::sys::host(&root, n)).unwrap(), std::path::Path::new("bindle"));
        }
    }

    #[test]
    fn module_recipes_need_the_module_installed() {
        let catalog = bindle_core::Catalog::arch().unwrap();
        let printing = catalog.modules.iter().find(|m| m.id == "printing").unwrap();
        assert!(printing.recipes.iter().any(|r| r.name == "printers"));
        let none = |_: &str| false;
        let all = |_: &str| true;
        let no_app = |_: &ReleaseAsset| false;
        assert!(module_installed(printing, &all, &no_app));
        assert!(!module_installed(printing, &|p| p != "cups", &no_app));
        assert!(!module_installed(&Module::default(), &all, &no_app), "installs nothing");
        assert!(!module_installed(printing, &none, &no_app));
    }

    #[test]
    fn clap_keeps_recipe_arguments_in_order() {
        let cli = BindoCli::parse_from(["bindo", "-n", "logs", "sshd", "20", "--yes"]);
        assert!(cli.dry_run);
        assert_eq!(cli.rest, ["logs", "sshd", "20", "--yes"]);
    }
}
