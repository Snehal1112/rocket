//! Rename hints: pair a removed operation with a new one that looks like the same thing.

use rocket_shared::types::HttpMethod;
use std::collections::{HashMap, HashSet};

/// What identifies an operation for a rename hint.
#[derive(Debug, Clone)]
pub(crate) struct RenameFacts {
    pub method: HttpMethod,
    /// The request URL, which holds the path template.
    pub url: String,
    pub name: String,
}

type Groups<'a> = HashMap<(&'a HttpMethod, &'a str), Vec<usize>>;

fn group<'a>(items: &'a [RenameFacts], by_name: bool) -> Groups<'a> {
    let mut groups: Groups = HashMap::new();
    for (i, f) in items.iter().enumerate() {
        let part = if by_name { &f.name } else { &f.url };
        groups
            .entry((&f.method, part.as_str()))
            .or_default()
            .push(i);
    }
    groups
}

/// Pairs `(removed index, new index)` when the match is unambiguous on both sides. Rule (a):
/// same method and path template. Rule (b): same method and request name. A candidate seen
/// ambiguously by either rule, or paired twice, gets no hint. Linear in the number of items.
pub(crate) fn pair_renames(removed: &[RenameFacts], new: &[RenameFacts]) -> Vec<(usize, usize)> {
    let mut pairs: HashSet<(usize, usize)> = HashSet::new();
    let mut poisoned_removed: HashSet<usize> = HashSet::new();
    let mut poisoned_new: HashSet<usize> = HashSet::new();

    for by_name in [false, true] {
        let removed_groups = group(removed, by_name);
        let new_groups = group(new, by_name);
        for (key, rs) in &removed_groups {
            let Some(ns) = new_groups.get(key) else {
                continue;
            };
            if let ([r], [n]) = (rs.as_slice(), ns.as_slice()) {
                pairs.insert((*r, *n));
            } else {
                poisoned_removed.extend(rs);
                poisoned_new.extend(ns);
            }
        }
    }

    let mut removed_count: HashMap<usize, usize> = HashMap::new();
    let mut new_count: HashMap<usize, usize> = HashMap::new();
    for (r, n) in &pairs {
        *removed_count.entry(*r).or_default() += 1;
        *new_count.entry(*n).or_default() += 1;
    }
    let mut out: Vec<(usize, usize)> = pairs
        .into_iter()
        .filter(|(r, n)| {
            !poisoned_removed.contains(r)
                && !poisoned_new.contains(n)
                && removed_count[r] == 1
                && new_count[n] == 1
        })
        .collect();
    out.sort_unstable();
    out
}
