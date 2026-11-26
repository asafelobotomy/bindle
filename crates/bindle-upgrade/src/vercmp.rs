//! pacman's version order (`vercmp`, alpm's `alpm_pkg_vercmp`) and package
//! relations (`name>=1.2`), so bindleUP can tell ahead of an AUR run whether
//! a new version's `conflicts` and `depends` can be met.

use std::cmp::Ordering;

/// pacman's order of two versions, `[epoch:]version[-release]`. The release
/// counts only when both have one, as in pacman.
pub fn vercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let (ea, va, ra) = split(a);
    let (eb, vb, rb) = split(b);
    rpmvercmp(ea, eb).then_with(|| rpmvercmp(va, vb)).then_with(|| match (ra, rb) {
        (Some(x), Some(y)) => rpmvercmp(x, y),
        _ => Ordering::Equal,
    })
}

fn split(s: &str) -> (&str, &str, Option<&str>) {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let (epoch, rest) = match s.as_bytes().get(digits) {
        Some(b':') => (&s[..digits], &s[digits + 1..]),
        _ => ("0", s),
    };
    match rest.rsplit_once('-') {
        Some((v, r)) => (epoch, v, Some(r)),
        None => (epoch, rest, None),
    }
}

/// rpm's segment-by-segment comparison, as pacman has it.
fn rpmvercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let (one, two) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < one.len() && j < two.len() {
        let (si, sj) = (i, j);
        while i < one.len() && !one[i].is_ascii_alphanumeric() {
            i += 1;
        }
        while j < two.len() && !two[j].is_ascii_alphanumeric() {
            j += 1;
        }
        if i >= one.len() || j >= two.len() {
            break;
        }
        // Different separator lengths decide it.
        if i - si != j - sj {
            return (i - si).cmp(&(j - sj));
        }
        let numeric = one[i].is_ascii_digit();
        let same = |c: u8| if numeric { c.is_ascii_digit() } else { c.is_ascii_alphabetic() };
        let (pi, pj) = (i, j);
        while i < one.len() && same(one[i]) {
            i += 1;
        }
        while j < two.len() && same(two[j]) {
            j += 1;
        }
        if pj == j {
            // A number beats letters; letters lose to a number.
            return if numeric { Ordering::Greater } else { Ordering::Less };
        }
        let (mut x, mut y) = (&one[pi..i], &two[pj..j]);
        if numeric {
            while x.len() > 1 && x[0] == b'0' {
                x = &x[1..];
            }
            while y.len() > 1 && y[0] == b'0' {
                y = &y[1..];
            }
            let by_len = x.len().cmp(&y.len());
            if by_len != Ordering::Equal {
                return by_len;
            }
        }
        let c = x.cmp(y);
        if c != Ordering::Equal {
            return c;
        }
    }
    let (rest_one, rest_two) = (&one[i.min(one.len())..], &two[j.min(two.len())..]);
    if rest_one.is_empty() && rest_two.is_empty() {
        return Ordering::Equal;
    }
    // A leftover letter segment never beats nothing: 1.0alpha < 1.0 < 1.0.1.
    let alpha_two = rest_two.first().is_some_and(u8::is_ascii_alphabetic);
    let alpha_one = rest_one.first().is_some_and(u8::is_ascii_alphabetic);
    if (rest_one.is_empty() && !alpha_two) || alpha_one { Ordering::Less } else { Ordering::Greater }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

impl Op {
    pub fn text(self) -> &'static str {
        match self {
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Eq => "=",
            Op::Ge => ">=",
            Op::Gt => ">",
        }
    }
}

/// One entry of `depends`, `conflicts` or `provides`: a name and maybe a
/// version bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    pub name: String,
    pub bound: Option<(Op, String)>,
}

impl Dep {
    /// `name`, `name>=1.2`, `name<2:1.0-3`; an optdepends' `: why` is dropped.
    pub fn parse(s: &str) -> Option<Dep> {
        let s = s.split(": ").next()?.trim();
        let at = s.find(['<', '>', '=']);
        let (name, rest) = match at {
            Some(k) => (&s[..k], &s[k..]),
            None => (s, ""),
        };
        if name.is_empty() || name.contains(char::is_whitespace) {
            return None;
        }
        let bound = match rest {
            "" => None,
            r => {
                let (op, v) = [(">=", Op::Ge), ("<=", Op::Le), (">", Op::Gt), ("<", Op::Lt), ("=", Op::Eq)]
                    .into_iter()
                    .find_map(|(t, op)| r.strip_prefix(t).map(|v| (op, v)))?;
                if v.is_empty() || v.contains(char::is_whitespace) || v.starts_with(['<', '>', '=']) {
                    return None;
                }
                Some((op, v.to_string()))
            }
        };
        Some(Dep { name: name.to_string(), bound })
    }

    /// Whether `version` of this name meets the bound (no bound: any).
    pub fn allows(&self, version: &str) -> bool {
        let Some((op, want)) = &self.bound else { return true };
        let c = vercmp(version, want);
        match op {
            Op::Lt => c == Ordering::Less,
            Op::Le => c != Ordering::Greater,
            Op::Eq => c == Ordering::Equal,
            Op::Ge => c != Ordering::Less,
            Op::Gt => c == Ordering::Greater,
        }
    }

    pub fn text(&self) -> String {
        match &self.bound {
            Some((op, v)) => format!("{}{}{v}", self.name, op.text()),
            None => self.name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::*;

    #[test]
    fn orders_like_pacman() {
        // Pairs from pacman's own vercmp tests.
        for (a, b, want) in [
            ("1.0", "1.0", Equal),
            ("1.0", "1.0.1", Less),
            ("1.0.1", "1.0", Greater),
            ("1.0a", "1.0", Less),
            ("1.0alpha", "1.0", Less),
            ("1.0", "1.0rc1", Greater),
            ("1.0rc1", "1.0", Less),
            ("1.0.a", "1.0", Greater),
            ("1.0.1", "1.0a", Greater),
            ("0.3.41", "0.3.9", Greater),
            ("0.3.41-1", "0.3.41-2", Less),
            ("0.3.41-1", "0.3.41", Equal),
            ("1:0.1", "2.0", Greater),
            ("1.001", "1.1", Equal),
            ("1.0.0", "1.0", Greater),
            ("1_0", "1.0", Equal),
            ("1.0..0", "1.0.0", Greater),
            ("1.5.b", "1.5", Greater),
            ("1.5b", "1.5", Less),
        ] {
            assert_eq!(vercmp(a, b), want, "{a} vs {b}");
            assert_eq!(vercmp(b, a), want.reverse(), "{b} vs {a}");
        }
    }

    #[test]
    fn reads_relations() {
        let d = Dep::parse("uq>=0.3.41").unwrap();
        assert_eq!((d.name.as_str(), d.bound.clone()), ("uq", Some((Op::Ge, "0.3.41".into()))));
        assert!(d.allows("0.3.42-1") && d.allows("0.3.41-3") && !d.allows("0.3.40-9"));
        let lt = Dep::parse("oysterav<0.5.0").unwrap();
        assert!(lt.allows("0.4.9-1") && !lt.allows("0.5.0-1"));
        assert_eq!(Dep::parse("xzram=0.7.6-1").unwrap().text(), "xzram=0.7.6-1");
        assert_eq!(Dep::parse("mpv: video").unwrap(), Dep { name: "mpv".into(), bound: None });
        for bad in ["", ">=1", "a b", "uq>=", "uq=>1"] {
            assert!(Dep::parse(bad).is_none(), "{bad}");
        }
    }
}
