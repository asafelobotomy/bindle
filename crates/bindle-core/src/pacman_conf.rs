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

/// Set `key` (or `key = value`) in `[options]`, replacing an existing or
/// commented-out line, otherwise appending to the end of the section.
pub fn set_option(text: &str, key: &str, value: &str) -> String {
    let rendered = if value.is_empty() { key.to_string() } else { format!("{key} = {value}") };
    let mut out: Vec<String> = Vec::new();
    let mut in_options = false;
    let mut done = false;
    let mut last_option_line = None;
    for line in text.lines() {
        if let Some(name) = section_of(line) {
            in_options = name == "options";
        } else if in_options && !done && option_key(line) == Some(key) {
            out.push(rendered.clone());
            done = true;
            continue;
        }
        if in_options && !line.trim().is_empty() {
            last_option_line = Some(out.len());
        }
        out.push(line.to_string());
    }
    if !done {
        match last_option_line {
            Some(i) => out.insert(i + 1, rendered),
            None => out.splice(0..0, ["[options]".to_string(), rendered]).for_each(drop),
        }
    }
    finish(out, text)
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

    #[test]
    fn enables_only_requested_repo() {
        let s = enable_repo(SAMPLE, "multilib");
        assert!(repo_enabled(&s, "multilib"));
        assert!(!repo_enabled(&s, "multilib-testing"));
        assert!(s.contains("[multilib]\nInclude = /etc/pacman.d/mirrorlist"));
        assert_eq!(enable_repo(&s, "multilib"), s);
    }
}
