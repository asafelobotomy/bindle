//! Running a recipe. Planning the command line ([`invocation`]) is pure;
//! [`run`] executes it with the terminal attached, so scripts (and pacman
//! inside them) can ask questions.
//!
//! Admin recipes go through polkit: `pkexec` (the desktop's password dialog,
//! or a text prompt on a console), else systemd's `run0`. Never sudo. The
//! script and its arguments travel as separate argv entries.

use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::recipe::Recipe;

/// How an admin recipe gets root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevate {
    /// Not an admin recipe, or bindo already runs as root.
    No,
    Pkexec,
    Run0,
}

/// The exact command a recipe run starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub recipe: String,
    pub argv: Vec<String>,
}

/// `[pkexec|run0] /bin/sh -ec <script> bindo-<name> <values...>`.
pub fn invocation(recipe: &Recipe, values: &[String], how: Elevate) -> Invocation {
    let mut argv: Vec<String> = match how {
        Elevate::No => vec![],
        Elevate::Pkexec => vec!["pkexec".into()],
        Elevate::Run0 => vec!["run0".into()],
    };
    argv.extend(["/bin/sh".into(), "-ec".into(), recipe.run.clone(), format!("bindo-{}", recipe.name)]);
    argv.extend(values.iter().cloned());
    Invocation { recipe: recipe.name.clone(), argv }
}

/// How this recipe would get root here, from what is installed.
pub fn elevation(recipe: &Recipe, is_root: bool, on_path: &dyn Fn(&str) -> bool) -> Result<Elevate> {
    if !recipe.admin || is_root {
        return Ok(Elevate::No);
    }
    if on_path("pkexec") {
        Ok(Elevate::Pkexec)
    } else if on_path("run0") {
        Ok(Elevate::Run0)
    } else {
        bail!("`{}` needs administrator rights, but neither pkexec (polkit) nor run0 is installed", recipe.name)
    }
}

/// Commands in `requires` that are not on PATH.
pub fn missing<'a>(recipe: &'a Recipe, on_path: &dyn Fn(&str) -> bool) -> Vec<&'a str> {
    recipe.requires.iter().map(String::as_str).filter(|c| !on_path(c)).collect()
}

pub fn on_path(prog: &str) -> bool {
    env::var_os("PATH").is_some_and(|p| env::split_paths(&p).any(|d| d.join(prog).is_file()))
}

pub fn is_root() -> bool {
    fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
}

/// Run it with the terminal attached; returns the exit code (128 + signal
/// when killed). With `dry_run`, only print the command.
pub fn run(inv: &Invocation, dry_run: bool) -> Result<i32> {
    if dry_run {
        println!("would run: {}", shell_words(&inv.argv));
        return Ok(0);
    }
    let (prog, args) = inv.argv.split_first().context("empty command")?;
    let status = Command::new(prog)
        .args(args)
        .env("BINDO_RECIPE", &inv.recipe)
        .status()
        .with_context(|| format!("start `{prog}`"))?;
    use std::os::unix::process::ExitStatusExt;
    Ok(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

/// A command line as it would be typed, quoting what needs it.
pub fn shell_words(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c)) {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Origin, parse};

    fn recipe(admin: bool) -> Recipe {
        let text = format!(
            "[[recipe]]\nname = \"r\"\ncategory = \"c\"\ndescription = \"d\"\nadmin = {admin}\nrequires = [\"paccache\", \"sh\"]\nrun = 'echo \"$1\"'\nargs = [{{ name = \"a\", kind = \"text\" }}]\n"
        );
        parse(&text, &Origin::Shipped).unwrap().remove(0)
    }

    #[test]
    fn values_are_positional_parameters_not_script_text() {
        let inv = invocation(&recipe(false), &["x; rm -rf ~".into()], Elevate::No);
        assert_eq!(inv.argv, ["/bin/sh", "-ec", "echo \"$1\"", "bindo-r", "x; rm -rf ~"]);
    }

    #[test]
    fn admin_recipes_use_polkit_never_sudo() {
        let all = |_: &str| true;
        let only_run0 = |p: &str| p == "run0";
        let none = |_: &str| false;
        assert_eq!(elevation(&recipe(true), false, &all).unwrap(), Elevate::Pkexec);
        assert_eq!(elevation(&recipe(true), false, &only_run0).unwrap(), Elevate::Run0);
        assert!(elevation(&recipe(true), false, &none).is_err());
        assert_eq!(elevation(&recipe(true), true, &none).unwrap(), Elevate::No);
        assert_eq!(elevation(&recipe(false), false, &all).unwrap(), Elevate::No);
        let inv = invocation(&recipe(true), &["v".into()], Elevate::Pkexec);
        assert_eq!(inv.argv[..2], ["pkexec", "/bin/sh"]);
        assert!(!inv.argv.iter().any(|a| a == "sudo"));
    }

    #[test]
    fn missing_requirements_are_named() {
        let r = recipe(false);
        assert_eq!(missing(&r, &|p| p == "sh"), ["paccache"]);
        assert!(missing(&r, &|_| true).is_empty());
    }

    #[test]
    fn dry_runs_print_and_succeed() {
        let inv = invocation(&recipe(true), &["a b".into()], Elevate::Pkexec);
        assert_eq!(run(&inv, true).unwrap(), 0);
        assert_eq!(shell_words(&inv.argv[4..]), "bindo-r 'a b'");
    }

    #[test]
    fn scripts_get_their_arguments() {
        let r = parse(
            "[[recipe]]\nname = \"t\"\ncategory = \"c\"\ndescription = \"d\"\nrun = 'test \"$1\" = \"a b\" && test \"$#\" = 1'\n",
            &Origin::Shipped,
        )
        .unwrap()
        .remove(0);
        assert_eq!(run(&invocation(&r, &["a b".into()], Elevate::No), false).unwrap(), 0);
        assert_eq!(run(&invocation(&r, &["c".into()], Elevate::No), false).unwrap(), 1);
    }
}
