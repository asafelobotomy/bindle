//! Word wrap with a hanging indent, so wrapped review lines stay under their
//! step number or bullet instead of falling back to column 0.

/// Where continuation lines start: after `12. [root] `, after `  • `/`  - `
/// style bullets, else under the line's own indentation.
fn indent(line: &str) -> usize {
    let lead = line.len() - line.trim_start().len();
    let rest = &line[lead..];
    if let Some(i) = rest.find("] ").filter(|_| rest.starts_with(|c: char| c.is_ascii_digit())) {
        return lead + i + 2;
    }
    let bullet = rest.chars().next().filter(|c| "•-~+✓".contains(*c));
    match bullet {
        Some(c) if rest[c.len_utf8()..].starts_with(' ') => lead + 2,
        _ if rest.starts_with("Modules: ") => 2,
        _ => lead,
    }
}

pub fn hanging(line: &str, width: usize) -> Vec<String> {
    let width = width.max(20);
    if line.chars().count() <= width {
        return vec![line.to_string()];
    }
    let pad = " ".repeat(indent(line).min(width / 2));
    let rest = line.trim_start();
    let mut out = Vec::new();
    let mut cur = line[..line.len() - rest.len()].to_string();
    for word in rest.split(' ') {
        let mut word: Vec<char> = word.chars().collect();
        let len = cur.chars().count();
        if !cur.trim().is_empty() {
            if len + 1 + word.len() <= width {
                cur.push(' ');
            } else {
                out.push(std::mem::replace(&mut cur, pad.clone()));
            }
        }
        // A word longer than a whole line (a long path) breaks mid-word.
        while cur.chars().count() + word.len() > width {
            let room = width - cur.chars().count();
            cur.extend(word.drain(..room));
            out.push(std::mem::replace(&mut cur, pad.clone()));
        }
        cur.extend(word);
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_lines_hang_under_the_text() {
        let l = " 3. [root] full system upgrade, then install (3): alpha beta gamma";
        assert_eq!(
            hanging(l, 40),
            [" 3. [root] full system upgrade, then", "           install (3): alpha beta gamma"]
        );
        let l = "  • zram: a note long enough to wrap onto the next line";
        assert_eq!(hanging(l, 30), ["  • zram: a note long enough", "    to wrap onto the next line"]);
        assert_eq!(hanging("short", 30), ["short"]);
        let l = " 4. [root] write /etc/a-very-long/path/that-does-not-fit.conf";
        assert!(hanging(l, 30).iter().all(|l| l.chars().count() <= 30));
    }
}
