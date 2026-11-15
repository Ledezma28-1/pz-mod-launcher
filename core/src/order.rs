use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use serde::Serialize;

use crate::modinfo::{ModInfo, normalize_id};
use crate::zb::FIXED_ORDER;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Rule {
    Requires,
    LoadAfter,
    LoadBefore,
    ZbFixed,
    UserPinned,
    FileRule,
    UserOverride,
}

impl Rule {
    pub fn is_hard(self) -> bool {
        !matches!(
            self,
            Rule::LoadAfter | Rule::LoadBefore | Rule::FileRule | Rule::UserOverride
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Rule,
    Winner,
    Category,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub ranges: Option<Vec<usize>>,
    pub extra: Vec<(String, String)>,
    pub overrides: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Edge {
    pub before: String,
    pub after: String,
    pub rule: Rule,
}

#[derive(Debug, Clone)]
pub struct Entry<'a> {
    pub id: String,
    pub enabled: bool,
    pub pinned: bool,
    pub info: Option<&'a ModInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cycle {
    pub mods: Vec<String>,
    pub edges: Vec<Edge>,
    pub hard: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Move {
    pub id: String,
    pub from: usize,
    pub to: usize,
    pub reasons: Vec<Edge>,
    pub effect: Effect,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderResult {
    pub order: Vec<String>,
    pub moves: Vec<Move>,
    pub cycles: Vec<Cycle>,
}

pub fn edges(entries: &[Entry]) -> Vec<Edge> {
    let active: Vec<&Entry> = entries.iter().filter(|e| e.enabled).collect();
    let ids: BTreeSet<String> = active.iter().map(|e| normalize_id(&e.id)).collect();
    let mut out = BTreeSet::new();
    let mut put = |a: &str, b: &str, rule: Rule| {
        let (a, b) = (normalize_id(a), normalize_id(b));
        if a != b && ids.contains(&a) && ids.contains(&b) {
            out.insert(Edge {
                before: a,
                after: b,
                rule,
            });
        }
    };
    for e in &active {
        let Some(i) = e.info else { continue };
        for r in &i.requires {
            put(r, &e.id, Rule::Requires);
        }
        for r in &i.load_after {
            put(r, &e.id, Rule::LoadAfter);
        }
        for r in &i.load_before {
            put(&e.id, r, Rule::LoadBefore);
        }
    }
    let fixed: Vec<&str> = FIXED_ORDER
        .iter()
        .copied()
        .filter(|f| ids.contains(*f))
        .collect();
    for (i, f) in fixed.iter().enumerate() {
        for e in &active {
            let id = normalize_id(&e.id);
            if !fixed[..=i].contains(&id.as_str()) {
                put(f, &id, Rule::ZbFixed);
            }
        }
    }
    let pins: Vec<&Entry> = active.iter().copied().filter(|e| e.pinned).collect();
    for par in pins.windows(2) {
        put(&par[0].id, &par[1].id, Rule::UserPinned);
    }
    out.into_iter().collect()
}

pub fn unmet_edges(order: &[String], edges: &[Edge]) -> Vec<Edge> {
    let pos: BTreeMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (normalize_id(id), i))
        .collect();
    edges
        .iter()
        .filter(|a| matches!((pos.get(&a.before), pos.get(&a.after)), (Some(x), Some(y)) if x > y))
        .cloned()
        .collect()
}

fn kahn(n: usize, suc: &[Vec<usize>], range: &[usize]) -> Vec<usize> {
    let mut degree = vec![0usize; n];
    for s in suc {
        for &y in s {
            degree[y] += 1;
        }
    }
    let mut free: BinaryHeap<Reverse<(usize, usize)>> = (0..n)
        .filter(|&i| degree[i] == 0)
        .map(|i| Reverse((range[i], i)))
        .collect();
    let mut new_order = Vec::with_capacity(n);
    while let Some(Reverse((_, v))) = free.pop() {
        new_order.push(v);
        for &w in &suc[v] {
            degree[w] -= 1;
            if degree[w] == 0 {
                free.push(Reverse((range[w], w)));
            }
        }
    }
    new_order
}

fn longest_increasing(seq: &[usize]) -> BTreeSet<usize> {
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; seq.len()];
    for (i, &x) in seq.iter().enumerate() {
        let k = tails.partition_point(|&t| seq[t] < x);
        if k > 0 {
            prev[i] = tails[k - 1];
        }
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = BTreeSet::new();
    let mut i = tails.last().copied().unwrap_or(usize::MAX);
    while i != usize::MAX {
        out.insert(seq[i]);
        i = prev[i];
    }
    out
}

fn components(n: usize, successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let (mut index, mut low, mut in_stack) = (vec![usize::MAX; n], vec![0; n], vec![false; n]);
    let (mut stack, mut out, mut counter) = (Vec::new(), Vec::new(), 0);
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut calls = vec![(root, 0usize)];
        while let Some(&mut (v, ref mut k)) = calls.last_mut() {
            if *k == 0 {
                index[v] = counter;
                low[v] = counter;
                counter += 1;
                stack.push(v);
                in_stack[v] = true;
            }
            if let Some(&w) = successors[v].get(*k) {
                *k += 1;
                if index[w] == usize::MAX {
                    calls.push((w, 0));
                } else if in_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                let mut c = Vec::new();
                while let Some(w) = stack.pop() {
                    in_stack[w] = false;
                    c.push(w);
                    if w == v {
                        break;
                    }
                }
                out.push(c);
            }
        }
    }
    out
}

pub fn sort(entries: &[Entry]) -> OrderResult {
    sort_with(entries, &Options::default())
}

pub fn sort_with(entries: &[Entry], options: &Options) -> OrderResult {
    let mut every = edges(entries);
    let active: Vec<String> = entries
        .iter()
        .filter(|e| e.enabled)
        .map(|e| normalize_id(&e.id))
        .collect();
    if !options.extra.is_empty() || !options.overrides.is_empty() {
        let mut together: BTreeSet<Edge> = every.into_iter().collect();
        let pairs = options
            .extra
            .iter()
            .map(|p| (p, Rule::FileRule))
            .chain(options.overrides.iter().map(|p| (p, Rule::UserOverride)));
        for ((a, b), rule) in pairs {
            let (a, b) = (normalize_id(a), normalize_id(b));
            if a != b && active.contains(&a) && active.contains(&b) {
                together.insert(Edge {
                    before: a,
                    after: b,
                    rule,
                });
            }
        }
        every = together.into_iter().collect();
    }
    let range: Vec<usize> = match &options.ranges {
        Some(r) => entries
            .iter()
            .zip(r)
            .filter(|(e, _)| e.enabled)
            .map(|(_, x)| *x)
            .collect(),
        None => (0..active.len()).collect(),
    };
    let n = active.len();
    let idx: BTreeMap<&str, usize> = active
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let par = |a: &Edge| (idx[a.before.as_str()], idx[a.after.as_str()]);

    let mut used: Vec<Edge> = every.clone();
    let mut cycles = Vec::new();
    let successors_of = |edges: &[Edge]| {
        let mut s = vec![Vec::new(); n];
        for a in edges {
            let (x, y) = par(a);
            s[x].push(y);
        }
        s
    };
    for comp in components(n, &successors_of(&used))
        .into_iter()
        .filter(|c| c.len() > 1)
    {
        let inside: BTreeSet<usize> = comp.iter().copied().collect();
        let internal_edges: Vec<Edge> = used
            .iter()
            .filter(|a| {
                let (x, y) = par(a);
                inside.contains(&x) && inside.contains(&y)
            })
            .cloned()
            .collect();
        let hard_ones: Vec<&Edge> = internal_edges.iter().filter(|a| a.rule.is_hard()).collect();
        let sub: Vec<Vec<usize>> = {
            let mut s = vec![Vec::new(); n];
            for a in &hard_ones {
                let (x, y) = par(a);
                s[x].push(y);
            }
            s
        };
        let hard = components(n, &sub)
            .iter()
            .any(|c| c.len() > 1 && c.iter().all(|v| inside.contains(v)));
        let mut mods: Vec<usize> = comp.clone();
        mods.sort();
        cycles.push(Cycle {
            mods: mods.iter().map(|&i| active[i].clone()).collect(),
            edges: internal_edges.clone(),
            hard,
        });
        used.retain(|a| {
            let (x, y) = par(a);
            let internal = inside.contains(&x) && inside.contains(&y);
            !internal || (a.rule.is_hard() && (!hard || x < y))
        });
    }

    let suc = successors_of(&used);
    let new_order = {
        let mut pred = vec![Vec::new(); n];
        for (x, s) in suc.iter().enumerate() {
            for &y in s {
                pred[y].push(x);
            }
        }
        for p in &mut pred {
            p.sort_by_key(|&x| (range[x], x));
        }
        let mut by_range: Vec<usize> = (0..n).collect();
        by_range.sort_by_key(|&x| (range[x], x));
        let mut slot = vec![false; n];
        let mut new_order = Vec::with_capacity(n);
        for root in by_range {
            let mut stack = vec![(root, 0usize)];
            while let Some(&mut (v, ref mut k)) = stack.last_mut() {
                if slot[v] {
                    stack.pop();
                    continue;
                }
                if let Some(&p) = pred[v].get(*k) {
                    *k += 1;
                    if !slot[p] {
                        stack.push((p, 0));
                    }
                    continue;
                }
                slot[v] = true;
                new_order.push(v);
                stack.pop();
            }
        }
        new_order
    };
    debug_assert_eq!(
        new_order.len(),
        n,
        "tras romper los ciclos el grafo es acíclico"
    );
    let mut slot_of = vec![0usize; n];
    for (i, &v) in new_order.iter().enumerate() {
        slot_of[v] = i;
    }
    let satisfies = suc
        .iter()
        .enumerate()
        .all(|(x, s)| s.iter().all(|&y| slot_of[x] < slot_of[y]));
    let new_order = if satisfies {
        new_order
    } else {
        kahn(n, &suc, &range)
    };

    let mut queue = new_order.into_iter();
    let order: Vec<String> = entries
        .iter()
        .map(|e| {
            if e.enabled {
                active[queue.next().expect("mismos activos")].clone()
            } else {
                normalize_id(&e.id)
            }
        })
        .collect();

    let before: BTreeMap<&str, usize> = active
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let after: BTreeMap<&str, usize> = order
        .iter()
        .filter(|id| before.contains_key(id.as_str()))
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let unmet = unmet_edges(&active, &used);
    let old_slots: Vec<usize> = order
        .iter()
        .filter_map(|id| before.get(id.as_str()).copied())
        .collect();
    let stay = longest_increasing(&old_slots);
    let moves = active
        .iter()
        .filter(|id| !stay.contains(&before[id.as_str()]))
        .map(|id| {
            let reasons: Vec<Edge> = unmet
                .iter()
                .filter(|a| &a.before == id || &a.after == id)
                .cloned()
                .collect();
            Move {
                id: id.clone(),
                from: before[id.as_str()],
                to: after[id.as_str()],
                effect: if reasons.is_empty() {
                    Effect::Category
                } else {
                    Effect::Rule
                },
                reasons,
            }
        })
        .collect();
    OrderResult {
        order,
        moves,
        cycles,
    }
}
