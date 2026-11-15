use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::diagnosis::EndKind;
use crate::modinfo::normalize_id;

pub type Requires = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Failed,
    Passed,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bisect {
    pub suspects: Vec<String>,
    pub keep: Vec<String>,
    pub testing: Vec<String>,
    #[serde(default)]
    pub units: Vec<Vec<String>>,
    #[serde(default)]
    pub requires: Requires,
    #[serde(default)]
    pub weights: BTreeMap<String, f64>,
    #[serde(default)]
    pub flaky: bool,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub rounds: Vec<(Vec<String>, Answer)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum Outcome {
    Culprit(String),
    Group(Vec<String>),
    Together(Vec<String>),
    NotTheMods,
    NotReproduced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Single,
    Together,
}

pub fn start(enabled: &[String], requires: &Requires) -> Bisect {
    start_with(enabled, requires, &BTreeMap::new(), false, None)
}

pub fn start_with(
    enabled: &[String],
    requires: &Requires,
    weights: &BTreeMap<String, f64>,
    flaky: bool,
    target: Option<String>,
) -> Bisect {
    let (keep, suspects): (Vec<String>, Vec<String>) = enabled
        .iter()
        .map(|m| normalize_id(m))
        .partition(|m| m == "ZombieBuddy");
    let inside: BTreeSet<&String> = suspects.iter().collect();
    let requires: Requires = suspects
        .iter()
        .filter_map(|m| {
            let r: Vec<String> = requires
                .get(m)?
                .iter()
                .filter(|r| inside.contains(r))
                .cloned()
                .collect();
            (!r.is_empty()).then(|| (m.clone(), r))
        })
        .collect();
    let mut b = Bisect {
        units: units_of(&suspects, &requires),
        suspects,
        keep,
        testing: Vec::new(),
        requires,
        weights: weights.clone(),
        flaky,
        target,
        rounds: Vec::new(),
    };
    refresh(&mut b);
    b
}

fn units_of(suspects: &[String], requires: &Requires) -> Vec<Vec<String>> {
    let reach = |from: &String| {
        let mut seen: BTreeSet<&String> = BTreeSet::new();
        let mut todo = vec![from];
        while let Some(m) = todo.pop() {
            for r in requires.get(m).into_iter().flatten() {
                if seen.insert(r) {
                    todo.push(r);
                }
            }
        }
        seen.into_iter().cloned().collect::<BTreeSet<String>>()
    };
    let reaches: BTreeMap<&String, BTreeSet<String>> =
        suspects.iter().map(|m| (m, reach(m))).collect();
    let mut placed: BTreeSet<&String> = BTreeSet::new();
    let mut out = Vec::new();
    for m in suspects {
        if !placed.insert(m) {
            continue;
        }
        let mut unit = vec![m.clone()];
        for o in suspects {
            if o != m && reaches[m].contains(o) && reaches[o].contains(m) && placed.insert(o) {
                unit.push(o.clone());
            }
        }
        out.push(unit);
    }
    out
}

fn deps_first(set: &[String], requires: &Requires) -> Vec<String> {
    let inside: BTreeSet<&String> = set.iter().collect();
    let mut seen: BTreeSet<&String> = BTreeSet::new();
    let mut out = Vec::with_capacity(set.len());
    for root in set {
        if seen.contains(root) {
            continue;
        }
        let mut stack: Vec<(&String, usize)> = vec![(root, 0)];
        seen.insert(root);
        while let Some((node, i)) = stack.pop() {
            let reqs = requires
                .get(node.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if let Some(r) = reqs.get(i) {
                stack.push((node, i + 1));
                if let Some(r) = inside.get(r)
                    && seen.insert(r)
                {
                    stack.push((r, 0));
                }
            } else {
                out.push(node.clone());
            }
        }
    }
    out
}

pub fn enabled_set(b: &Bisect, requires: &Requires) -> Vec<String> {
    let was_on: BTreeSet<&String> = b.keep.iter().chain(&b.suspects).collect();
    let mut on: BTreeSet<String> = BTreeSet::new();
    let mut todo: Vec<&String> = b.keep.iter().chain(&b.testing).collect();
    while let Some(m) = todo.pop() {
        if on.insert(m.clone()) {
            todo.extend(
                requires
                    .get(m.as_str())
                    .into_iter()
                    .flatten()
                    .filter(|r| was_on.contains(r)),
            );
        }
    }
    on.into_iter().collect()
}

pub fn answer(b: &mut Bisect, a: Answer) {
    if result(b).is_some() {
        return;
    }
    b.rounds.push((b.testing.clone(), a));
    refresh(b);
}

pub fn undo(b: &mut Bisect) -> bool {
    let done = b.rounds.pop().is_some();
    refresh(b);
    done
}

pub fn result(b: &Bisect) -> Option<Outcome> {
    replay(b).outcome
}

pub fn mode(b: &Bisect) -> Mode {
    replay(b).mode
}

pub fn remaining(b: &Bisect) -> usize {
    replay(b).remaining
}

pub fn rounds_left(b: &Bisect) -> u32 {
    let r = replay(b);
    if r.outcome.is_some() {
        return 0;
    }
    let n = r.alive.max(1) as u32;
    let halves = n.next_power_of_two().trailing_zeros() + 1;
    match r.mode {
        Mode::Single if b.flaky => halves * 2,
        Mode::Single => halves,
        Mode::Together => halves * 2,
    }
}

pub fn suggest(target: Option<&str>, end: EndKind, signature: Option<&str>) -> Option<Answer> {
    let target = target?;
    match (end, signature) {
        (EndKind::StoppedByUser, _) => None,
        (EndKind::Normal, _) => Some(Answer::Passed),
        (_, Some(s)) if s == target => Some(Answer::Failed),
        (_, Some(_)) => Some(Answer::Skip),
        (_, None) => None,
    }
}

pub fn refresh(b: &mut Bisect) {
    let next = replay(b).next;
    let on: BTreeSet<&String> = next.iter().collect();
    b.testing = b
        .suspects
        .iter()
        .filter(|m| on.contains(m))
        .cloned()
        .collect();
}

struct Replay {
    mode: Mode,
    outcome: Option<Outcome>,
    next: Vec<String>,
    alive: usize,
    remaining: usize,
}

fn rates(flaky: bool) -> (f64, f64) {
    if flaky { (0.7, 0.02) } else { (1.0, 0.0) }
}

const SURE: f64 = 0.95;
const MAX_ROUNDS: usize = 40;

fn replay(b: &Bisect) -> Replay {
    let units: Vec<Vec<String>> = if b.units.is_empty() {
        b.suspects.iter().map(|m| vec![m.clone()]).collect()
    } else {
        b.units.clone()
    };
    let n = units.len();
    if n == 0 {
        return Replay {
            mode: Mode::Single,
            outcome: Some(Outcome::NotTheMods),
            next: Vec::new(),
            alive: 0,
            remaining: 0,
        };
    }
    let (p, eps) = rates(b.flaky);
    let (not_mods, none) = (n, n + 1);
    let weight = |u: &Vec<String>| {
        u.iter()
            .map(|m| b.weights.get(m).copied().unwrap_or(1.0).max(0.0))
            .fold(0.0, f64::max)
            .max(1e-9)
    };
    let total: f64 = units.iter().map(weight).sum();
    let mut post: Vec<f64> = units.iter().map(|u| 0.9 * weight(u) / total).collect();
    post.push(0.05);
    post.push(0.05);
    let on_units = |set: &[String]| -> Vec<bool> {
        let s: BTreeSet<&String> = set.iter().collect();
        units
            .iter()
            .map(|u| u.iter().any(|m| s.contains(m)))
            .collect()
    };
    let fail_rate = |h: usize, on: &[bool]| {
        if h == none {
            eps
        } else if h == not_mods || on[h] {
            p
        } else {
            eps
        }
    };
    let mut full_passed = false;
    let mut full_failed = false;
    let mut failed_rounds: Vec<Vec<bool>> = Vec::new();
    let mut skipped: Vec<Vec<String>> = Vec::new();
    let mut answered = 0;
    for (i, (set, a)) in b.rounds.iter().enumerate() {
        let failed = match a {
            Answer::Skip => {
                skipped.push(set.clone());
                continue;
            }
            Answer::Failed => true,
            Answer::Passed => false,
        };
        answered += 1;
        let on = on_units(set);
        if on.iter().all(|x| *x) {
            full_passed |= !failed;
            full_failed |= failed;
        }
        if failed {
            failed_rounds.push(on.clone());
        }
        for (h, q) in post.iter_mut().enumerate() {
            let f = fail_rate(h, &on);
            *q *= if failed { f } else { 1.0 - f };
        }
        let sum: f64 = post.iter().sum();
        if sum <= 0.0 {
            return together(b, &units, &b.rounds[i + 1..], full_failed);
        }
        post.iter_mut().for_each(|q| *q /= sum);
    }
    let alive_units: Vec<usize> = (0..n).filter(|&k| post[k] > 1e-12).collect();
    let remaining = alive_units.iter().map(|&k| units[k].len()).sum();
    let alive = post.iter().filter(|q| **q > 1e-12).count();
    let best = (0..post.len())
        .max_by(|&x, &y| post[x].total_cmp(&post[y]))
        .unwrap_or(none);
    let decided = if b.flaky {
        post[best] >= SURE || answered >= MAX_ROUNDS
    } else {
        alive == 1
    };
    if decided && best == none && !full_passed {
        return Replay {
            mode: Mode::Single,
            outcome: None,
            next: b.suspects.clone(),
            alive,
            remaining,
        };
    }
    if decided && best < n {
        let alone = closure_units(b, &units, best);
        let confirmed = failed_rounds
            .iter()
            .any(|on| on.iter().enumerate().all(|(k, x)| !x || alone.contains(&k)));
        if !confirmed {
            let on: BTreeSet<usize> = alone.into_iter().collect();
            return Replay {
                mode: Mode::Single,
                outcome: None,
                next: b
                    .suspects
                    .iter()
                    .filter(|m| {
                        units
                            .iter()
                            .enumerate()
                            .any(|(k, u)| on.contains(&k) && u.contains(m))
                    })
                    .cloned()
                    .collect(),
                alive,
                remaining,
            };
        }
    }
    if decided {
        let outcome = if best == none {
            Outcome::NotReproduced
        } else if best == not_mods {
            Outcome::NotTheMods
        } else if units[best].len() == 1 {
            Outcome::Culprit(units[best][0].clone())
        } else {
            Outcome::Group(units[best].clone())
        };
        return Replay {
            mode: Mode::Single,
            outcome: Some(outcome),
            next: Vec::new(),
            alive,
            remaining,
        };
    }
    Replay {
        mode: Mode::Single,
        outcome: None,
        next: choose(b, &units, &post, &skipped, p, eps),
        alive,
        remaining,
    }
}

fn closure_units(b: &Bisect, units: &[Vec<String>], u: usize) -> Vec<usize> {
    let unit_of: BTreeMap<&String, usize> = units
        .iter()
        .enumerate()
        .flat_map(|(k, x)| x.iter().map(move |m| (m, k)))
        .collect();
    let mut out = BTreeSet::from([u]);
    let mut todo: Vec<&String> = units[u].iter().collect();
    while let Some(m) = todo.pop() {
        for r in b.requires.get(m).into_iter().flatten() {
            if let Some(&k) = unit_of.get(r)
                && out.insert(k)
            {
                todo.extend(units[k].iter());
            }
        }
    }
    out.into_iter().collect()
}

fn entropy(v: &[f64]) -> f64 {
    let s: f64 = v.iter().sum();
    if s <= 0.0 {
        return 0.0;
    }
    v.iter()
        .filter(|q| **q > 0.0)
        .map(|q| {
            let q = q / s;
            -q * q.ln()
        })
        .sum()
}

fn choose(
    b: &Bisect,
    units: &[Vec<String>],
    post: &[f64],
    skipped: &[Vec<String>],
    p: f64,
    eps: f64,
) -> Vec<String> {
    let n = units.len();
    let (not_mods, none) = (n, n + 1);
    let mut by_post: Vec<usize> = (0..n).collect();
    by_post.sort_by(|&x, &y| post[y].total_cmp(&post[x]).then(x.cmp(&y)));
    let mods: Vec<String> = by_post.iter().flat_map(|&k| units[k].clone()).collect();
    let order = deps_first(&mods, &b.requires);
    let unit_of: BTreeMap<&String, usize> = units
        .iter()
        .enumerate()
        .flat_map(|(k, u)| u.iter().map(move |m| (m, k)))
        .collect();
    let mut best: Option<(f64, usize)> = None;
    let mut prefix: BTreeSet<&String> = BTreeSet::new();
    let mut missing: BTreeSet<&String> = BTreeSet::new();
    let mut in_unit = vec![0usize; n];
    let mut split = 0usize;
    let mut on = vec![false; n];
    let set_of = |prefix: &BTreeSet<&String>| -> Vec<String> {
        b.suspects
            .iter()
            .filter(|m| prefix.contains(m))
            .cloned()
            .collect()
    };
    for k in 0..=order.len() {
        if k > 0 {
            let m = &order[k - 1];
            prefix.insert(m);
            missing.remove(m);
            for r in b.requires.get(m.as_str()).into_iter().flatten() {
                if !prefix.contains(r) {
                    missing.insert(r);
                }
            }
            let u = unit_of[m];
            in_unit[u] += 1;
            on[u] = true;
            if in_unit[u] == 1 && units[u].len() > 1 {
                split += 1;
            }
            if in_unit[u] == units[u].len() && units[u].len() > 1 {
                split -= 1;
            }
        }
        if !missing.is_empty() || split > 0 {
            continue;
        }
        if !skipped.is_empty() && skipped.contains(&set_of(&prefix)) {
            continue;
        }
        let rate = |h: usize| {
            if h == none {
                eps
            } else if h == not_mods || on[h] {
                p
            } else {
                eps
            }
        };
        let fail: Vec<f64> = post.iter().enumerate().map(|(h, q)| q * rate(h)).collect();
        let pass: Vec<f64> = post
            .iter()
            .enumerate()
            .map(|(h, q)| q * (1.0 - rate(h)))
            .collect();
        let (pf, pp): (f64, f64) = (fail.iter().sum(), pass.iter().sum());
        if pf <= 1e-12 || pp <= 1e-12 {
            continue;
        }
        let expected = pf * entropy(&fail) + pp * entropy(&pass);
        let better = match &best {
            None => true,
            Some((e, size)) => expected < e - 1e-12 || (expected <= e + 1e-12 && k > *size),
        };
        if better {
            best = Some((expected, k));
        }
    }
    match best {
        Some((_, k)) => {
            let chosen: BTreeSet<&String> = order[..k].iter().collect();
            set_of(&chosen)
        }
        None => b.suspects.clone(),
    }
}

#[derive(Clone)]
struct Frame {
    fixed: Vec<usize>,
    a: Vec<usize>,
    b: Vec<usize>,
    stage: u8,
    got_a: Vec<usize>,
}

enum Step {
    Test(Vec<usize>),
    Done(Vec<usize>),
}

fn halves(b: &Bisect, units: &[Vec<String>], cand: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let mods: Vec<String> = cand.iter().flat_map(|&k| units[k].clone()).collect();
    let order = deps_first(&mods, &b.requires);
    let unit_of: BTreeMap<&String, usize> = cand
        .iter()
        .flat_map(|&k| units[k].iter().map(move |m| (m, k)))
        .collect();
    let mut seen = BTreeSet::new();
    let ordered: Vec<usize> = order
        .iter()
        .map(|m| unit_of[m])
        .filter(|k| seen.insert(*k))
        .collect();
    let half = ordered.len().div_ceil(2);
    (ordered[..half].to_vec(), ordered[half..].to_vec())
}

fn union(x: &[usize], y: &[usize]) -> Vec<usize> {
    let mut v: Vec<usize> = x.iter().chain(y).copied().collect();
    v.sort();
    v.dedup();
    v
}

fn open(
    b: &Bisect,
    units: &[Vec<String>],
    fixed: Vec<usize>,
    cand: &[usize],
) -> Result<Frame, Vec<usize>> {
    if cand.len() <= 1 {
        return Err(cand.to_vec());
    }
    let (a, bb) = halves(b, units, cand);
    Ok(Frame {
        fixed,
        a,
        b: bb,
        stage: 0,
        got_a: Vec::new(),
    })
}

fn deliver(
    b: &Bisect,
    units: &[Vec<String>],
    stack: &mut Vec<Frame>,
    mut found: Vec<usize>,
) -> Step {
    loop {
        let Some(f) = stack.last_mut() else {
            return Step::Done(found);
        };
        if f.stage == 2 {
            f.got_a = found;
            f.stage = 3;
            let (fixed, bb) = (union(&f.fixed, &f.got_a), f.b.clone());
            match open(b, units, fixed, &bb) {
                Ok(child) => {
                    stack.push(child);
                    return next_test(stack);
                }
                Err(r) => {
                    found = r;
                    continue;
                }
            }
        }
        let f = stack.pop().expect("frame");
        found = union(&f.got_a, &found);
    }
}

fn next_test(stack: &[Frame]) -> Step {
    let f = stack.last().expect("frame");
    match f.stage {
        0 => Step::Test(union(&f.fixed, &f.a)),
        _ => Step::Test(union(&f.fixed, &f.b)),
    }
}

fn together(
    b: &Bisect,
    units: &[Vec<String>],
    rounds: &[(Vec<String>, Answer)],
    all_failed: bool,
) -> Replay {
    let all: Vec<usize> = (0..units.len()).collect();
    let to_mods = |set: &[usize]| -> Vec<String> {
        let on: BTreeSet<&String> = set.iter().flat_map(|&k| units[k].iter()).collect();
        b.suspects
            .iter()
            .filter(|m| on.contains(m))
            .cloned()
            .collect()
    };
    let finish = |found: Vec<usize>| {
        let mods = to_mods(&found);
        let outcome = match (found.len(), mods.len()) {
            (1, 1) => Outcome::Culprit(mods[0].clone()),
            (1, _) => Outcome::Group(mods),
            _ => Outcome::Together(mods),
        };
        Replay {
            mode: Mode::Together,
            outcome: Some(outcome),
            next: Vec::new(),
            alive: 0,
            remaining: 0,
        }
    };
    let mut answers = rounds
        .iter()
        .filter(|(_, a)| *a != Answer::Skip)
        .map(|(_, a)| *a);
    match if all_failed {
        Some(Answer::Failed)
    } else {
        answers.next()
    } {
        None => {
            return Replay {
                mode: Mode::Together,
                outcome: None,
                next: b.suspects.clone(),
                alive: units.len(),
                remaining: b.suspects.len(),
            };
        }
        Some(Answer::Passed) => {
            return Replay {
                mode: Mode::Together,
                outcome: Some(Outcome::NotReproduced),
                next: Vec::new(),
                alive: 0,
                remaining: 0,
            };
        }
        _ => {}
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut step = match open(b, units, Vec::new(), &all) {
        Ok(f) => {
            stack.push(f);
            next_test(&stack)
        }
        Err(found) => Step::Done(found),
    };
    loop {
        match step {
            Step::Done(found) => return finish(found),
            Step::Test(set) => {
                let Some(a) = answers.next() else {
                    let alive = stack.last().map_or(0, |f| f.a.len() + f.b.len());
                    let remaining =
                        to_mods(&stack.last().map_or(Vec::new(), |f| union(&f.a, &f.b))).len();
                    return Replay {
                        mode: Mode::Together,
                        outcome: None,
                        next: to_mods(&set),
                        alive,
                        remaining,
                    };
                };
                let failed = a == Answer::Failed;
                let f = stack.last_mut().expect("frame");
                step = match (f.stage, failed) {
                    (0, true) | (1, true) => {
                        let part = if f.stage == 0 {
                            f.a.clone()
                        } else {
                            f.b.clone()
                        };
                        let fixed = f.fixed.clone();
                        stack.pop();
                        match open(b, units, fixed, &part) {
                            Ok(child) => {
                                stack.push(child);
                                next_test(&stack)
                            }
                            Err(found) => deliver(b, units, &mut stack, found),
                        }
                    }
                    (0, false) => {
                        f.stage = 1;
                        next_test(&stack)
                    }
                    _ => {
                        f.stage = 2;
                        let (fixed, a) = (union(&f.fixed, &f.b), f.a.clone());
                        match open(b, units, fixed, &a) {
                            Ok(child) => {
                                stack.push(child);
                                next_test(&stack)
                            }
                            Err(found) => deliver(b, units, &mut stack, found),
                        }
                    }
                };
            }
        }
    }
}
