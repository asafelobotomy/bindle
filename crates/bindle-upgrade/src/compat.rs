//! Partner versions for the AUR stage: an AUR update is held back when its
//! new version's versioned `conflicts` or `depends` can't be met by what will
//! be installed (a partner app isn't on the AUR at the version it needs yet),
//! and the rest are installed in an order pacman accepts.
//!
//! uQ and its apps declare the versions they work with as pacman relations
//! (`conflicts+=('uq<0.3.41')`, uQ's `compat/family.toml`); pacman refuses a
//! breaking set on its own, but only once it is built and half installed.
//! Looking first means the run never stops on it: the app waits for its
//! partner, and everything else still updates.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::parse::Update;
use crate::vercmp::Dep;

/// One package: its version and the relations that carry a version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Pkg {
    pub version: String,
    #[serde(skip)]
    pub depends: Vec<Dep>,
    #[serde(skip)]
    pub conflicts: Vec<Dep>,
    #[serde(skip)]
    pub provides: Vec<Dep>,
}

/// What the probe found: the installed foreign packages (`pacman -Qmi`) and
/// the AUR's new versions of the ones with updates (the AUR's info API).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Facts {
    pub installed: BTreeMap<String, Pkg>,
    pub candidates: BTreeMap<String, Pkg>,
    /// Why the partners couldn't be checked; pacman still refuses a bad set.
    pub problem: Option<String>,
}

/// An update that waits, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Held {
    pub name: String,
    pub reason: String,
}

/// One AUR run of the stage: these packages, together in one pacman
/// transaction when `together` (they need each other's new versions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Wave {
    pub names: Vec<String>,
    pub together: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub held: Vec<Held>,
    /// None: one run of all of them is fine (no order needed).
    pub waves: Option<Vec<Wave>>,
}

struct View<'a> {
    facts: &'a Facts,
    going: BTreeSet<&'a str>,
}

impl<'a> View<'a> {
    fn new_of(&self, n: &str) -> Option<&'a Pkg> {
        self.going.contains(n).then(|| self.facts.candidates.get(n)).flatten()
    }
    /// The package as it will be: its new version when it updates.
    fn after(&self, n: &str) -> Option<&'a Pkg> {
        self.new_of(n).or_else(|| self.facts.installed.get(n))
    }
    fn names(&self) -> BTreeSet<&'a str> {
        self.facts.installed.keys().map(String::as_str).chain(self.going.iter().copied()).collect()
    }
}

/// The version under which `pkg` (called `own`) answers to `name`: its own
/// version, or a versioned `provides`.
fn answers<'p>(own: &str, pkg: &'p Pkg, name: &str) -> Option<&'p str> {
    if own == name {
        return Some(&pkg.version);
    }
    pkg.provides.iter().find(|p| p.name == name).and_then(|p| p.bound.as_ref()).map(|(_, v)| v.as_str())
}

fn shown(n: &str, p: &Pkg) -> String {
    format!("{n} {}", p.version)
}

/// Who of `n` and `t` waits when the two can't be installed together: the
/// one whose update brings the clash; never one that isn't updating.
fn pick<'a>(v: &View<'a>, n: &'a str, t: &'a str) -> Option<&'a str> {
    [n, t].into_iter().find(|x| v.going.contains(x))
}

/// Clashes in the set as it will be, each as (who waits, why).
fn clashes<'a>(v: &View<'a>) -> Vec<(&'a str, String)> {
    let names = v.names();
    let mut out = vec![];
    for &n in &names {
        let Some(p) = v.after(n) else { continue };
        for d in p.conflicts.iter().filter(|d| d.bound.is_some()) {
            for &t in names.iter().filter(|&&t| t != n) {
                let Some(q) = v.after(t) else { continue };
                let Some(ver) = answers(t, q, &d.name) else { continue };
                if !d.allows(ver) {
                    continue;
                }
                let Some(who) = pick(v, n, t) else { continue };
                let why = if who == n {
                    let (_, want) = d.bound.as_ref().unwrap();
                    format!(
                        "{} doesn't work with {} ({}); it waits for {} {want}",
                        shown(n, p),
                        shown(t, q),
                        d.text(),
                        t
                    )
                } else {
                    format!("{} would break {}, which has no update for it yet", shown(t, q), shown(n, p))
                };
                out.push((who, why));
            }
        }
        for d in p.depends.iter().filter(|d| d.bound.is_some()) {
            let found: Vec<&str> =
                names.iter().filter(|&&t| t != n).filter_map(|&t| answers(t, v.after(t)?, &d.name)).collect();
            if found.is_empty() || found.iter().any(|ver| d.allows(ver)) {
                // Not one of the installed AUR packages (pacman's), or met.
                continue;
            }
            if let Some(who) = pick(v, n, &d.name) {
                let why = format!("{} needs {}, which isn't on the AUR yet", shown(n, p), d.text());
                out.push((who, why));
            }
        }
    }
    out
}

/// Whether one of `rels` (versioned) names `to` at its version.
fn hit(rels: &[Dep], to_name: &str, to: &Pkg) -> bool {
    rels.iter().filter(|d| d.bound.is_some()).any(|d| answers(to_name, to, &d.name).is_some_and(|v| d.allows(v)))
}

/// Whether installing `b`'s new version while `a` still has its old one
/// would clash, so `a` has to be updated first.
fn before(facts: &Facts, a: &str, b: &str) -> bool {
    let (Some(a_old), Some(b_new)) = (facts.installed.get(a), facts.candidates.get(b)) else { return false };
    let unmet = b_new
        .depends
        .iter()
        .filter(|d| d.bound.is_some())
        .any(|d| answers(a, a_old, &d.name).is_some_and(|v| !d.allows(v)));
    hit(&b_new.conflicts, a, a_old) || hit(&a_old.conflicts, b, b_new) || unmet
}

/// Which updates wait, and in what runs the rest go in.
pub fn check(facts: &Facts, updates: &[Update]) -> Verdict {
    let mut held: Vec<Held> = vec![];
    // An update the AUR said nothing about can't be judged: it goes.
    let names: BTreeSet<&str> = updates.iter().map(|u| u.name.as_str()).collect();
    loop {
        let waiting: BTreeSet<&str> = held.iter().map(|h| h.name.as_str()).collect();
        let v = View { facts, going: names.iter().copied().filter(|n| !waiting.contains(n)).collect() };
        let mut more = false;
        for (who, why) in clashes(&v) {
            if !held.iter().any(|h| h.name == who) {
                held.push(Held { name: who.to_string(), reason: why });
                more = true;
            }
        }
        if !more {
            let going: Vec<&str> = v.going.iter().copied().collect();
            return Verdict { waves: waves(facts, &going), held };
        }
    }
}

/// The runs, providers first; None when no update has to wait for another.
fn waves(facts: &Facts, going: &[&str]) -> Option<Vec<Wave>> {
    let n = going.len();
    // reach[i][j]: i has to go in no later than j.
    let mut reach = vec![vec![false; n]; n];
    let mut any = false;
    for i in 0..n {
        for j in 0..n {
            if i != j && before(facts, going[i], going[j]) {
                reach[i][j] = true;
                any = true;
            }
        }
    }
    if !any {
        return None;
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                if reach[i][k] && reach[k][j] {
                    reach[i][j] = true;
                }
            }
        }
    }
    let mut left: BTreeSet<usize> = (0..n).collect();
    let mut out = vec![];
    while !left.is_empty() {
        // Ready: nothing left has to go before it, other than its own cycle.
        let ready: Vec<usize> =
            left.iter().copied().filter(|&j| left.iter().all(|&i| i == j || !reach[i][j] || reach[j][i])).collect();
        let together = ready.iter().any(|&j| ready.iter().any(|&i| i != j && reach[i][j]));
        out.push(Wave { names: ready.iter().map(|&j| going[j].to_string()).collect(), together });
        for j in ready {
            left.remove(&j);
        }
    }
    Some(out)
}
