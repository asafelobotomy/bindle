//! The recipe format: TOML, one `[[recipe]]` table per recipe.
//!
//! ```toml
//! [[recipe]]
//! name = "clean-journal"
//! category = "Maintenance"
//! description = "Shrink the system journal to a size"
//! admin = true
//! args = [{ name = "size", kind = "size", default = "500M" }]
//! run = 'journalctl --vacuum-size="$1"'
//! ```
//!
//! The script runs with `sh -e`; its arguments are `$1`, `$2`, … in the
//! order of `args`.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Longest one-line description the listing shows unwrapped.
pub const MAX_DESCRIPTION: usize = 72;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub name: String,
    pub category: String,
    /// One line for the listing.
    pub description: String,
    /// More detail for `bindo <recipe> --help`.
    #[serde(default)]
    pub help: String,
    /// Runs as root, after a polkit password prompt.
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub args: Vec<Arg>,
    /// Commands the script needs on PATH; the listing says which are missing.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Asked (y/N) before running, unless `--yes`.
    #[serde(default)]
    pub confirm: Option<String>,
    /// The `sh` script.
    pub run: String,
    #[serde(skip)]
    pub origin: Origin,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Arg {
    pub name: String,
    #[serde(default)]
    pub help: String,
    /// Optional when set.
    #[serde(default)]
    pub default: Option<String>,
    /// The only values accepted, when not empty.
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub kind: Kind,
    /// Takes every remaining argument (zero or more); last only.
    #[serde(default)]
    pub rest: bool,
}

/// What an argument may hold. None of them may start with `-`, so a value
/// can never be read as an option by the commands it reaches.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A name: letters, digits and `._+-:@`.
    #[default]
    Word,
    /// Digits only.
    Number,
    /// Digits with an optional K, M or G (journal and cache sizes).
    Size,
    /// Any single line of text.
    Text,
}

impl Kind {
    pub fn accepts(self, v: &str) -> bool {
        if v.is_empty() || v.starts_with('-') {
            return false;
        }
        match self {
            Kind::Word => v.chars().all(|c| c.is_ascii_alphanumeric() || "._+-:@".contains(c)),
            Kind::Number => v.len() <= 9 && v.chars().all(|c| c.is_ascii_digit()),
            Kind::Size => {
                let digits = v.strip_suffix(['K', 'M', 'G']).unwrap_or(v);
                !digits.is_empty() && digits.len() <= 9 && digits.chars().all(|c| c.is_ascii_digit())
            }
            Kind::Text => !v.chars().any(char::is_control),
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Kind::Word => "a name",
            Kind::Number => "a number",
            Kind::Size => "a size such as 500M or 2G",
            Kind::Text => "text",
        }
    }
}

/// Where a recipe came from; later origins override earlier ones by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Origin {
    /// Built into bindo.
    #[default]
    Shipped,
    /// A Bindle catalog module that is installed (its name).
    Module(String),
    /// A file in a recipe directory.
    File(PathBuf),
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Shipped => write!(f, "shipped with Bindle"),
            Origin::Module(m) => write!(f, "Bindle's {m} module"),
            Origin::File(p) => write!(f, "{}", p.display()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    recipe: Vec<Recipe>,
}

/// Parse and check one recipe file.
pub fn parse(text: &str, origin: &Origin) -> Result<Vec<Recipe>, String> {
    let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for mut r in file.recipe {
        r.validate().map_err(|e| format!("recipe `{}`: {e}", r.name))?;
        if !seen.insert(r.name.clone()) {
            return Err(format!("recipe `{}` is defined twice", r.name));
        }
        r.origin = origin.clone();
        out.push(r);
    }
    Ok(out)
}

fn valid_recipe_name(n: &str) -> bool {
    n.len() <= 32
        && n.starts_with(|c: char| c.is_ascii_lowercase())
        && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !n.ends_with('-')
}

impl Recipe {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_recipe_name(&self.name) {
            return Err("names are lowercase letters, digits and `-`, up to 32".into());
        }
        if self.category.trim().is_empty() || self.category.contains('\n') {
            return Err("needs a one-line category".into());
        }
        let d = self.description.trim();
        if d.is_empty() || d.contains('\n') || d.chars().count() > MAX_DESCRIPTION {
            return Err(format!("needs a one-line description of at most {MAX_DESCRIPTION} characters"));
        }
        if self.run.trim().is_empty() {
            return Err("has nothing to run".into());
        }
        if let Some(r) = self.requires.iter().find(|r| r.is_empty() || !Kind::Word.accepts(r)) {
            return Err(format!("`requires` entry `{r}` is not a command name"));
        }
        self.validate_args()
    }

    fn validate_args(&self) -> Result<(), String> {
        let mut optional = false;
        for (i, a) in self.args.iter().enumerate() {
            let ident = a.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if a.name.is_empty() || !ident {
                return Err(format!("argument `{}` needs a lowercase name", a.name));
            }
            if self.args[..i].iter().any(|b| b.name == a.name) {
                return Err(format!("argument `{}` appears twice", a.name));
            }
            if a.rest && i + 1 != self.args.len() {
                return Err(format!("only the last argument can take the rest (`{}`)", a.name));
            }
            if a.rest && a.default.is_some() {
                return Err(format!("`{}` takes the rest, so it can't have a default", a.name));
            }
            if let Some(c) = a.choices.iter().find(|c| !a.kind.accepts(c)) {
                return Err(format!("choice `{c}` of `{}` is not {}", a.name, a.kind.describe()));
            }
            if let Some(d) = &a.default
                && !a.accepts(d)
            {
                return Err(format!("default `{d}` of `{}` is not accepted by it", a.name));
            }
            let required = a.default.is_none() && !a.rest;
            if required && optional {
                return Err(format!("required argument `{}` follows an optional one", a.name));
            }
            optional |= !required;
        }
        Ok(())
    }

    /// `name <required> [optional] [rest...]`, as the listing shows it.
    pub fn signature(&self) -> String {
        let mut s = self.name.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(&match (a.rest, a.default.is_some()) {
                (true, _) => format!("[{}...]", a.name),
                (false, true) => format!("[{}]", a.name),
                (false, false) => format!("<{}>", a.name),
            });
        }
        s
    }
}

impl Arg {
    pub fn accepts(&self, v: &str) -> bool {
        if self.choices.is_empty() { self.kind.accepts(v) } else { self.choices.iter().any(|c| c == v) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(extra: &str) -> Result<Vec<Recipe>, String> {
        let text =
            format!("[[recipe]]\nname = \"x\"\ncategory = \"System\"\ndescription = \"d\"\nrun = \"true\"\n{extra}");
        parse(&text, &Origin::Shipped)
    }

    #[test]
    fn kinds_never_accept_options() {
        for k in [Kind::Word, Kind::Number, Kind::Size, Kind::Text] {
            assert!(!k.accepts("-rf"), "{k:?}");
            assert!(!k.accepts(""), "{k:?}");
        }
        assert!(Kind::Size.accepts("500M") && Kind::Size.accepts("12") && !Kind::Size.accepts("M"));
        assert!(Kind::Number.accepts("3") && !Kind::Number.accepts("3a"));
        assert!(Kind::Word.accepts("sshd.service") && !Kind::Word.accepts("a b") && !Kind::Word.accepts("a;b"));
        assert!(Kind::Text.accepts("hello world") && !Kind::Text.accepts("a\nb"));
    }

    #[test]
    fn a_plain_recipe_parses() {
        let r = one("").unwrap();
        assert_eq!(r[0].name, "x");
        assert!(!r[0].admin && r[0].args.is_empty());
    }

    #[test]
    fn unknown_keys_and_bad_names_are_refused() {
        assert!(one("sudo = true").is_err());
        let bad = "[[recipe]]\nname = \"Bad Name\"\ncategory = \"c\"\ndescription = \"d\"\nrun = \"true\"\n";
        assert!(parse(bad, &Origin::Shipped).unwrap_err().contains("lowercase"));
    }

    #[test]
    fn argument_order_and_defaults_are_checked() {
        let e = one("args = [{ name = \"a\", default = \"1\" }, { name = \"b\" }]").unwrap_err();
        assert!(e.contains("follows an optional"), "{e}");
        let e = one("args = [{ name = \"a\", rest = true }, { name = \"b\" }]").unwrap_err();
        assert!(e.contains("last argument"), "{e}");
        let e = one("args = [{ name = \"a\", choices = [\"on\"], default = \"off\" }]").unwrap_err();
        assert!(e.contains("default `off`"), "{e}");
        let e = one("args = [{ name = \"a\", kind = \"number\", default = \"x\" }]").unwrap_err();
        assert!(e.contains("default `x`"), "{e}");
    }

    #[test]
    fn signatures_show_what_is_optional() {
        let r =
            one("args = [{ name = \"unit\" }, { name = \"n\", default = \"1\" }, { name = \"more\", rest = true }]")
                .unwrap();
        assert_eq!(r[0].signature(), "x <unit> [n] [more...]");
    }

    #[test]
    fn descriptions_stay_one_short_line() {
        assert!(one("").is_ok());
        let long = format!(
            "[[recipe]]\nname = \"x\"\ncategory = \"c\"\ndescription = \"{}\"\nrun = \"true\"\n",
            "a".repeat(80)
        );
        assert!(parse(&long, &Origin::Shipped).is_err());
    }
}
