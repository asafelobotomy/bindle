//! A line diff for the small config files modules write: `- old`, `+ new`
//! and two lines of context around each change; longer unchanged runs fold
//! into `…`.

/// Lines of `old` and `new`, diffed (LCS).
pub fn diff(old: &str, new: &str) -> Vec<String> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut ops: Vec<(char, &str)> = vec![];
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            ops.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            ops.push(('-', a[i]));
            i += 1;
        } else {
            ops.push(('+', b[j]));
            j += 1;
        }
    }
    let near = |k: usize| ops.iter().enumerate().any(|(x, (c, _))| *c != ' ' && x.abs_diff(k) <= 2);
    let mut out = vec![];
    for (k, (c, line)) in ops.iter().enumerate() {
        if *c != ' ' || near(k) {
            out.push(format!("{c} {line}"));
        } else if out.last().is_none_or(|l| l != "…") {
            out.push("…".into());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_lines_with_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nd\nE\nf\ng\nh\ni\n";
        assert_eq!(diff(old, new), ["…", "  c", "  d", "- e", "+ E", "  f", "  g", "  h", "+ i"]);
        assert!(diff("same\n", "same\n").iter().all(|l| l == "…"));
        assert_eq!(diff("", "x\n"), ["+ x"]);
    }
}
