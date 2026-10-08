//! Minimal, comment-preserving edits to /etc/pacman.conf.

fn section_of(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[')?.strip_suffix(']')
}

/// True when `[repo]` appears uncommented.
pub fn repo_enabled(text: &str, repo: &str) -> bool {
    text.lines().any(|l| section_of(l) == Some(repo))
}

/// Uncomment `#[repo]` and the commented directives directly beneath it.
pub fn enable_repo(text: &str, repo: &str) -> String {
    if repo_enabled(text, repo) {
        return text.to_string();
    }
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#')
            && section_of(rest.trim()) == Some(repo)
        {
            inside = true;
            out.push(rest.trim().to_string());
            continue;
        }
        if inside {
            match trimmed.strip_prefix('#').map(str::trim) {
                Some(d) if d.contains('=') && !d.starts_with('[') => {
                    out.push(d.to_string());
                    continue;
                }
                _ => inside = false,
            }
        }
        out.push(line.to_string());
    }
    if !repo_enabled(&out.join("\n"), repo) {
        out.push(String::new());
        out.push(format!("[{repo}]"));
        out.push("Include = /etc/pacman.d/mirrorlist".into());
    }
    finish(out, text)
}

fn option_key(line: &str) -> Option<&str> {
    let body = line.trim().trim_start_matches('#').trim();
    let key = body.split(['=', ' ', '\t']).next()?;
    (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric())).then_some(key)
}

/// Value of an active `Key = Value` / `Key=Value` / bare `Key` line.
fn option_value(line: &str) -> &str {
    let body = line.trim();
    let rest = body.split_once(['=', ' ', '\t']).map_or("", |(_, r)| r).trim_start();
    rest.strip_prefix('=').unwrap_or(rest).trim()
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

/// Set `key` (or `key = value`) in `[options]`. An active line wins over a
/// commented one; later active duplicates are commented out; an equivalent
/// line (`Key=Val`) is left alone. A missing option goes after the last
/// active directive of the section, never into a trailing commented block.
pub fn set_option(text: &str, key: &str, value: &str) -> String {
    let rendered = if value.is_empty() { key.to_string() } else { format!("{key} = {value}") };
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut active = Vec::new();
    let mut commented = None;
    let mut header = None;
    let mut last_directive = None;
    let mut in_options = false;
    // Commented directives after a commented `#[repo]` header belong to it.
    let mut in_commented_repo = false;
    for (i, line) in lines.iter().enumerate() {
        if let Some(name) = section_of(line) {
            in_options = name == "options";
            in_commented_repo = false;
            if in_options {
                header = Some(i);
            }
            continue;
        }
        if !in_options {
            continue;
        }
        if is_comment(line) {
            let body = line.trim_start().trim_start_matches('#').trim();
            in_commented_repo |= section_of(body).is_some();
            if !in_commented_repo && commented.is_none() && option_key(line) == Some(key) {
                commented = Some(i);
            }
        } else if let Some(k) = option_key(line) {
            last_directive = Some(i);
            if k == key {
                active.push(i);
            }
        }
    }
    if let Some((&first, rest)) = active.split_first() {
        if option_value(&lines[first]) != value {
            lines[first] = rendered;
        }
        for &i in rest {
            lines[i] = format!("#{}", lines[i]);
        }
    } else if let Some(i) = commented {
        lines[i] = rendered;
    } else if let Some(i) = last_directive.or(header) {
        lines.insert(i + 1, rendered);
    } else {
        lines.splice(0..0, ["[options]".to_string(), rendered]);
    }
    finish(lines, text)
}

fn finish(lines: Vec<String>, original: &str) -> String {
    let mut s = lines.join("\n");
    if original.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[options]
HoldPkg     = pacman glibc
#Color
#VerbosePkgLists
ParallelDownloads = 5

[core]
Include = /etc/pacman.d/mirrorlist

#[multilib-testing]
#Include = /etc/pacman.d/mirrorlist

#[multilib]
#Include = /etc/pacman.d/mirrorlist
";

    #[test]
    fn uncomments_flag_and_replaces_value() {
        let s = set_option(SAMPLE, "Color", "");
        let s = set_option(&s, "ParallelDownloads", "10");
        assert!(s.contains("\nColor\n"));
        assert!(s.contains("ParallelDownloads = 10"));
        assert!(!s.contains("ParallelDownloads = 5"));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn appends_missing_option_inside_options() {
        let s = set_option(SAMPLE, "ILoveCandy", "");
        let opts = s.split("[core]").next().unwrap();
        assert!(opts.contains("ILoveCandy"));
    }

    /// Stock Arch /etc/pacman.conf, abridged to the parts that matter.
    const STOCK: &str = "\
#
# /etc/pacman.conf
#
[options]
# The following paths are commented out with their default values listed.
#RootDir     = /
HoldPkg     = pacman glibc
#XferCommand = /usr/bin/curl -L -C - -f -o %o %u
#XferCommand = /usr/bin/wget --passive-ftp -c -O %o %u
Architecture = auto

# Misc options
#UseSyslog
#Color
#NoProgressBar
CheckSpace
#VerbosePkgLists
ParallelDownloads = 5
DownloadUser = alpm
#DisableSandbox

SigLevel    = Required DatabaseOptional
LocalFileSigLevel = Optional
#RemoteFileSigLevel = Required

#
# REPOSITORIES
#       [repo-name]
#       Include = IncludePath
#

#[core-testing]
#Include = /etc/pacman.d/mirrorlist

[core]
Include = /etc/pacman.d/mirrorlist
";

    #[test]
    fn stock_layout_edits() {
        let s = set_option(STOCK, "Color", "");
        assert!(s.contains("#UseSyslog\nColor\n#NoProgressBar"));
        let s = set_option(&s, "ILoveCandy", "");
        assert!(s.contains("LocalFileSigLevel = Optional\nILoveCandy\n#RemoteFileSigLevel"));
        // Enabling core-testing later must not swallow the new option.
        let t = enable_repo(&s, "core-testing");
        assert!(t.contains("[core-testing]\nInclude = /etc/pacman.d/mirrorlist\n\n[core]"));
        assert_eq!(set_option(&s, "ILoveCandy", ""), s);
        assert_eq!(set_option(&s, "Color", ""), s);
    }

    #[test]
    fn prefers_active_line_and_comments_duplicates() {
        let conf = "[options]\n#ParallelDownloads = 5\nParallelDownloads=3\nColor\nParallelDownloads = 8\n";
        let s = set_option(conf, "ParallelDownloads", "10");
        assert_eq!(s, "[options]\n#ParallelDownloads = 5\nParallelDownloads = 10\nColor\n#ParallelDownloads = 8\n");
        let same = "[options]\nParallelDownloads=10\n";
        assert_eq!(set_option(same, "ParallelDownloads", "10"), same);
        assert_eq!(set_option("[core]\n", "Color", ""), "[options]\nColor\n[core]\n");
    }

    #[test]
    fn enables_only_requested_repo() {
        let s = enable_repo(SAMPLE, "multilib");
        assert!(repo_enabled(&s, "multilib"));
        assert!(!repo_enabled(&s, "multilib-testing"));
        assert!(s.contains("[multilib]\nInclude = /etc/pacman.d/mirrorlist"));
        assert_eq!(enable_repo(&s, "multilib"), s);
    }
}
