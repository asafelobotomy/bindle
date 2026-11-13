//! Text bindo prints: the listing, one recipe's help, shell completions.
//! Plain lines, so tests can read them.

use crate::book::Book;
use crate::exec::missing;
use crate::recipe::Recipe;

pub const ADMIN: &str = "[admin]";

/// Every recipe by category with its one-line description. Admin recipes
/// are marked; recipes missing a command say which.
pub fn list(book: &Book, on_path: &dyn Fn(&str) -> bool) -> Vec<String> {
    let groups = book.by_category();
    let width = groups.iter().flat_map(|(_, rs)| rs.iter().map(|r| r.signature().len())).max().unwrap_or(0);
    let mut out = vec!["Usage: bindo <recipe> [args]    bindo <recipe> --help for details".to_string()];
    for (category, recipes) in groups {
        out.push(String::new());
        out.push(format!("{category}:"));
        for r in recipes {
            let mut line = format!("  {:<width$}  {}", r.signature(), r.description);
            if r.admin {
                line.push_str("  ");
                line.push_str(ADMIN);
            }
            let gone = missing(r, on_path);
            if !gone.is_empty() {
                line.push_str(&format!("  (needs {})", gone.join(", ")));
            }
            out.push(line);
        }
    }
    out.push(String::new());
    out.push(format!("{ADMIN} recipes run as root and ask for your password first."));
    out
}

/// `bindo <recipe> --help`: usage, details, arguments, where it comes from
/// and the script itself, so nothing runs unseen.
pub fn help(r: &Recipe, on_path: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = vec![format!("Usage: bindo {}", r.signature()), String::new(), r.description.clone()];
    if !r.help.trim().is_empty() {
        out.push(String::new());
        out.extend(r.help.trim().lines().map(String::from));
    }
    if !r.args.is_empty() {
        out.push(String::new());
        out.push("Arguments:".into());
        for a in &r.args {
            let mut line = format!("  {:<12}", a.name);
            if !a.help.is_empty() {
                line.push_str(&format!(" {}", a.help));
            }
            if !a.choices.is_empty() {
                line.push_str(&format!(" [{}]", a.choices.join(", ")));
            }
            if let Some(d) = &a.default {
                line.push_str(&format!(" (default: {d})"));
            }
            out.push(line.trim_end().to_string());
        }
    }
    out.push(String::new());
    if r.admin {
        out.push(format!("{ADMIN} Runs as root: asks for your password (polkit) first."));
    }
    if let Some(q) = &r.confirm {
        out.push(format!("Asks first: {q} (--yes skips this)"));
    }
    let gone = missing(r, on_path);
    if !gone.is_empty() {
        out.push(format!("Needs {}, which is not installed.", gone.join(", ")));
    }
    out.push(format!("From: {}", r.origin));
    out.push("Runs (sh -e, arguments as $1, $2, …):".into());
    out.extend(r.run.trim_end().lines().map(|l| format!("    {l}")));
    out
}

/// A completion script for `shell`. Recipe names and argument choices come
/// from bindo at completion time, so user recipes complete too.
pub fn completions(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        _ => None,
    }
}

const BASH: &str = r#"# bindo completion for bash
_bindo() {
    local cur=${COMP_WORDS[COMP_CWORD]} i words=()
    for ((i = 1; i < COMP_CWORD; i++)); do
        [[ ${COMP_WORDS[i]} == -* ]] || words+=("${COMP_WORDS[i]}")
    done
    if [[ $cur == -* ]]; then
        COMPREPLY=($(compgen -W "--help --yes --dry-run --list" -- "$cur"))
    elif ((${#words[@]} == 0)); then
        COMPREPLY=($(compgen -W "$(bindo --names 2>/dev/null)" -- "$cur"))
    else
        COMPREPLY=($(compgen -W "$(bindo --choices "${words[0]}" $((${#words[@]} - 1)) 2>/dev/null)" -- "$cur"))
    fi
}
complete -F _bindo bindo
"#;

const ZSH: &str = r#"#compdef bindo
# bindo completion for zsh: save as _bindo in a directory on $fpath.
local -a plain
plain=(${words[2,CURRENT-1]:#-*})
if (( ${#plain} == 0 )); then
    compadd -- ${(f)"$(bindo --names 2>/dev/null)"}
else
    compadd -- ${(f)"$(bindo --choices ${plain[1]} $(( ${#plain} - 1 )) 2>/dev/null)"}
fi
"#;

const FISH: &str = r#"# bindo completion for fish
function __bindo_args
    set -l words (commandline -opc)[2..-1]
    set -l plain
    for w in $words
        string match -q -- '-*' $w; or set -a plain $w
    end
    if test (count $plain) -eq 0
        bindo --names 2>/dev/null
    else
        bindo --choices $plain[1] (math (count $plain) - 1) 2>/dev/null
    end
end
complete -c bindo -f -a '(__bindo_args)'
complete -c bindo -s y -l yes -d "Don't ask before running"
complete -c bindo -s n -l dry-run -d 'Print the command instead of running it'
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Book {
        Book::shipped().unwrap()
    }

    #[test]
    fn listing_groups_marks_admin_and_missing_commands() {
        let lines = list(&book(), &|p| p != "paccache");
        let at = |s: &str| lines.iter().position(|l| l == s).unwrap_or_else(|| panic!("no `{s}` in {lines:#?}"));
        assert!(at("System:") < at("Maintenance:") && at("Maintenance:") < at("Toggles:"));
        let journal = lines.iter().find(|l| l.trim_start().starts_with("clean-journal")).unwrap();
        assert!(journal.contains(ADMIN), "{journal}");
        let cache = lines.iter().find(|l| l.trim_start().starts_with("clean-pkg-cache")).unwrap();
        assert!(cache.ends_with("(needs paccache)"), "{cache}");
        let info = lines.iter().find(|l| l.trim_start().starts_with("info")).unwrap();
        assert!(!info.contains(ADMIN));
    }

    #[test]
    fn descriptions_line_up() {
        let lines = list(&book(), &|_| true);
        let cols: Vec<usize> = lines
            .iter()
            .filter(|l| l.starts_with("  "))
            .map(|l| {
                let sig_end = l[2..].find("  ").unwrap() + 2;
                sig_end + l[sig_end..].len() - l[sig_end..].trim_start().len()
            })
            .collect();
        assert!(cols.windows(2).all(|w| w[0] == w[1]), "{cols:?}");
    }

    #[test]
    fn help_shows_arguments_and_the_script() {
        let b = book();
        let lines = help(b.get("clean-journal").unwrap(), &|_| true);
        assert_eq!(lines[0], "Usage: bindo clean-journal [size]");
        assert!(lines.iter().any(|l| l.contains("(default: 500M)")));
        assert!(lines.iter().any(|l| l.starts_with(ADMIN)));
        assert!(lines.iter().any(|l| l.contains("journalctl --vacuum-size")));
        assert!(lines.iter().any(|l| l == "From: shipped with Bindle"));
    }

    #[test]
    fn completions_exist_for_common_shells() {
        for sh in ["bash", "zsh", "fish"] {
            let s = completions(sh).unwrap();
            assert!(s.contains("bindo --names") && s.contains("--choices"), "{sh}");
        }
        assert!(completions("tcsh").is_none());
    }
}
