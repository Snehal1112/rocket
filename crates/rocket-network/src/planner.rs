//! The three-way planner. Pure: no I/O, no clock.

use std::collections::{BTreeMap, BTreeSet};

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::link::{path_is_valid, Manifest};

/// What the planning node does. "Mine" is the planning node, "theirs" the other one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Pull { path: String },
    PullAs { path: String, as_path: String },
    Push { path: String },
    DeleteMine { path: String },
    DeleteTheirs { path: String },
    RenameMine { from: String, to: String },
    RenameTheirs { from: String, to: String },
}

/// Why a path needs a decision from the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    BothChanged,
    DeleteVsEdit,
    BothAdded,
    RenameVsEdit,
}

/// A path both sides changed in different ways. Hashes are `None` when the file is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub kind: ConflictKind,
    pub mine: Option<String>,
    pub theirs: Option<String>,
}

/// What the user chose for one conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    KeepMine,
    KeepTheirs,
    KeepBoth,
}

/// The choice for the conflict at `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub path: String,
    pub resolution: Resolution,
}

/// The result of the three-way compare: changes that apply on their own, and conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncPlan {
    pub actions: Vec<SyncAction>,
    pub conflicts: Vec<Conflict>,
}

type Map = BTreeMap<String, String>;

fn renames_on(base: &Map, side: &Map) -> Vec<(String, String)> {
    // Added paths by hash, each list in path order, so the first unused one is the smallest.
    let mut added: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
    for (path, hash) in side.iter().filter(|(p, _)| !base.contains_key(*p)) {
        added.entry(hash.as_str()).or_default().push(path);
    }
    let mut next: BTreeMap<&str, usize> = BTreeMap::new();
    let mut pairs = Vec::new();
    for (from, hash) in base.iter().filter(|(p, _)| !side.contains_key(*p)) {
        let Some(paths) = added.get(hash.as_str()) else {
            continue;
        };
        let at = next.entry(hash.as_str()).or_insert(0);
        if let Some(to) = paths.get(*at) {
            *at += 1;
            pairs.push((from.clone(), (*to).clone()));
        }
    }
    pairs
}

/// Plans a sync from the planning node's view: `mine` is its manifest, `theirs` the starter's.
///
/// Renames are paired first, then each remaining path goes through the three-way table. The
/// result is sorted by path. A `RenameMine` or `RenameTheirs` whose `from` is the path of a
/// `RenameVsEdit` conflict is a marker only: `resolve` replaces it.
pub fn plan_sync(base: &Manifest, mine: &Manifest, theirs: &Manifest) -> SyncPlan {
    let (b, m, t) = (base.entries(), mine.entries(), theirs.entries());
    let mut items: Vec<(String, SyncAction)> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();
    let mut consumed: BTreeSet<String> = BTreeSet::new();

    for theirs_renamed in [true, false] {
        let (renamer, other) = if theirs_renamed { (t, m) } else { (m, t) };
        for (from, to) in renames_on(b, renamer) {
            if other.contains_key(&to) {
                continue;
            }
            let Some(other_hash) = other.get(&from) else {
                continue;
            };
            consumed.insert(from.clone());
            consumed.insert(to.clone());
            let action = if theirs_renamed {
                SyncAction::RenameMine {
                    from: from.clone(),
                    to: to.clone(),
                }
            } else {
                SyncAction::RenameTheirs {
                    from: from.clone(),
                    to: to.clone(),
                }
            };
            if b.get(&from) != Some(other_hash) {
                let (mine_hash, theirs_hash) = if theirs_renamed {
                    (Some(other_hash.clone()), renamer.get(&to).cloned())
                } else {
                    (renamer.get(&to).cloned(), Some(other_hash.clone()))
                };
                conflicts.push(Conflict {
                    path: from.clone(),
                    kind: ConflictKind::RenameVsEdit,
                    mine: mine_hash,
                    theirs: theirs_hash,
                });
            }
            items.push((from, action));
        }
    }

    let paths: BTreeSet<&String> = b.keys().chain(m.keys()).chain(t.keys()).collect();
    for path in paths {
        if consumed.contains(path) {
            continue;
        }
        let (bh, mh, th) = (b.get(path), m.get(path), t.get(path));
        if mh == th {
            continue;
        }
        let p = path.clone();
        if bh == mh {
            items.push((
                p.clone(),
                if th.is_some() {
                    SyncAction::Pull { path: p }
                } else {
                    SyncAction::DeleteMine { path: p }
                },
            ));
        } else if bh == th {
            items.push((
                p.clone(),
                if mh.is_some() {
                    SyncAction::Push { path: p }
                } else {
                    SyncAction::DeleteTheirs { path: p }
                },
            ));
        } else {
            let kind = if bh.is_none() {
                ConflictKind::BothAdded
            } else if mh.is_none() || th.is_none() {
                ConflictKind::DeleteVsEdit
            } else {
                ConflictKind::BothChanged
            };
            conflicts.push(Conflict {
                path: p,
                kind,
                mine: mh.cloned(),
                theirs: th.cloned(),
            });
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    conflicts.sort_by(|a, b| a.path.cmp(&b.path));
    SyncPlan {
        actions: items.into_iter().map(|(_, a)| a).collect(),
        conflicts,
    }
}

fn keep_both_path(path: &str, taken: &Manifest, chosen: &BTreeSet<String>) -> DomainResult<String> {
    let (dir, name) = match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path),
    };
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    let mut n = 1u32;
    loop {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" {n}")
        };
        let candidate = format!("{dir}{stem} (from peer){suffix}{ext}");
        if !taken.has_folded(&candidate) && !chosen.contains(&candidate.to_lowercase()) {
            return if path_is_valid(&candidate) {
                Ok(candidate)
            } else {
                Err(DomainError::InvalidInput(
                    "keep both path is not valid".into(),
                ))
            };
        }
        n += 1;
    }
}

fn pull_or_delete(has: bool, path: &str) -> SyncAction {
    if has {
        SyncAction::Pull { path: path.into() }
    } else {
        SyncAction::DeleteMine { path: path.into() }
    }
}
fn push_or_delete(has: bool, path: &str) -> SyncAction {
    if has {
        SyncAction::Push { path: path.into() }
    } else {
        SyncAction::DeleteTheirs { path: path.into() }
    }
}

/// Turns a plan and the user's decisions into the final actions.
///
/// Every conflict needs exactly one decision. `taken` must hold every path present on either
/// side, so a keep-both name never collides with a file on the other node.
pub fn resolve(
    plan: &SyncPlan,
    decisions: &[Decision],
    taken: &Manifest,
) -> DomainResult<Vec<SyncAction>> {
    let mut by_path: BTreeMap<&str, Resolution> = BTreeMap::new();
    for d in decisions {
        if !plan.conflicts.iter().any(|c| c.path == d.path) {
            return Err(DomainError::InvalidInput(
                "decision for unknown conflict".into(),
            ));
        }
        if by_path.insert(d.path.as_str(), d.resolution).is_some() {
            return Err(DomainError::InvalidInput("duplicate decision".into()));
        }
    }
    for c in &plan.conflicts {
        if !by_path.contains_key(c.path.as_str()) {
            return Err(DomainError::InvalidInput("missing decision".into()));
        }
    }
    let rename_conflicts: BTreeSet<&str> = plan
        .conflicts
        .iter()
        .filter(|c| c.kind == ConflictKind::RenameVsEdit)
        .map(|c| c.path.as_str())
        .collect();
    let is_provisional = |a: &SyncAction| matches!(a, SyncAction::RenameMine { from, .. } | SyncAction::RenameTheirs { from, .. } if rename_conflicts.contains(from.as_str()));
    let mut out: Vec<SyncAction> = plan
        .actions
        .iter()
        .filter(|a| !is_provisional(a))
        .cloned()
        .collect();
    let mut chosen = BTreeSet::new();
    for c in &plan.conflicts {
        let Some(resolution) = by_path.get(c.path.as_str()).copied() else {
            continue;
        };
        let path = c.path.as_str();
        match c.kind {
            ConflictKind::RenameVsEdit => {
                let renamed = plan.actions.iter().find_map(|a| match a {
                    SyncAction::RenameMine { from, to } if from == path => Some((true, to.clone())),
                    SyncAction::RenameTheirs { from, to } if from == path => {
                        Some((false, to.clone()))
                    }
                    _ => None,
                });
                let Some((theirs_renamed, to)) = renamed else {
                    return Err(DomainError::InvalidInput(
                        "rename conflict without rename".into(),
                    ));
                };
                let q = to.as_str();
                let del_mine = |p: &str| SyncAction::DeleteMine { path: p.into() };
                let del_theirs = |p: &str| SyncAction::DeleteTheirs { path: p.into() };
                let pull = |p: &str| SyncAction::Pull { path: p.into() };
                let push = |p: &str| SyncAction::Push { path: p.into() };
                let actions = match (theirs_renamed, resolution) {
                    (true, Resolution::KeepMine) => vec![push(path), del_theirs(q)],
                    (true, Resolution::KeepTheirs) => vec![del_mine(path), pull(q)],
                    (true, Resolution::KeepBoth) => vec![push(path), pull(q)],
                    (false, Resolution::KeepMine) => vec![del_theirs(path), push(q)],
                    (false, Resolution::KeepTheirs) => vec![pull(path), del_mine(q)],
                    (false, Resolution::KeepBoth) => vec![pull(path), push(q)],
                };
                out.extend(actions);
            }
            _ => match resolution {
                Resolution::KeepMine => out.push(push_or_delete(c.mine.is_some(), path)),
                Resolution::KeepTheirs => out.push(pull_or_delete(c.theirs.is_some(), path)),
                Resolution::KeepBoth => match (c.mine.is_some(), c.theirs.is_some()) {
                    (true, true) => {
                        let as_path = keep_both_path(path, taken, &chosen)?;
                        chosen.insert(as_path.to_lowercase());
                        out.push(SyncAction::RenameTheirs {
                            from: path.into(),
                            to: as_path.clone(),
                        });
                        out.push(SyncAction::PullAs {
                            path: path.into(),
                            as_path,
                        });
                        out.push(SyncAction::Push { path: path.into() });
                    }
                    (true, false) => out.push(SyncAction::Push { path: path.into() }),
                    _ => out.push(SyncAction::Pull { path: path.into() }),
                },
            },
        }
    }
    Ok(out)
}

/// The manifest both sides store after a successful apply, seen from the planning node.
///
/// `base` and `decisions` are part of the contract but unused: the final actions already carry
/// every decision, and the agreed state is `mine` with the planning node's own actions applied.
pub fn new_base(
    _base: &Manifest,
    mine: &Manifest,
    theirs: &Manifest,
    actions: &[SyncAction],
    _decisions: &[Decision],
) -> Manifest {
    let mut result: Map = mine.entries().clone();
    for action in actions {
        if let SyncAction::RenameMine { from, to } = action {
            if let Some(hash) = result.remove(from) {
                result.insert(to.clone(), hash);
            }
        }
    }
    for action in actions {
        match action {
            SyncAction::DeleteMine { path } => {
                result.remove(path);
            }
            SyncAction::Pull { path } => {
                if let Some(h) = theirs.entries().get(path) {
                    result.insert(path.clone(), h.clone());
                }
            }
            SyncAction::PullAs { path, as_path } => {
                if let Some(h) = theirs.entries().get(path) {
                    result.insert(as_path.clone(), h.clone());
                }
            }
            _ => {}
        }
    }
    Manifest::from_entries(result)
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A 64 character hash made of one repeated hex digit.
    fn h(digit: char) -> String {
        digit.to_string().repeat(64)
    }

    fn manifest(entries: &[(&str, char)]) -> Manifest {
        let mut manifest = Manifest::new();
        for (path, digit) in entries {
            manifest.insert(path, &h(*digit)).expect("valid entry");
        }
        manifest
    }

    fn pull(path: &str) -> SyncAction {
        SyncAction::Pull { path: path.into() }
    }
    fn push(path: &str) -> SyncAction {
        SyncAction::Push { path: path.into() }
    }
    fn delete_mine(path: &str) -> SyncAction {
        SyncAction::DeleteMine { path: path.into() }
    }
    fn delete_theirs(path: &str) -> SyncAction {
        SyncAction::DeleteTheirs { path: path.into() }
    }
    fn rename_mine(from: &str, to: &str) -> SyncAction {
        SyncAction::RenameMine {
            from: from.into(),
            to: to.into(),
        }
    }
    fn rename_theirs(from: &str, to: &str) -> SyncAction {
        SyncAction::RenameTheirs {
            from: from.into(),
            to: to.into(),
        }
    }
    fn conflict(
        path: &str,
        kind: ConflictKind,
        mine: Option<char>,
        theirs: Option<char>,
    ) -> Conflict {
        Conflict {
            path: path.into(),
            kind,
            mine: mine.map(h),
            theirs: theirs.map(h),
        }
    }
    fn decide(path: &str, resolution: Resolution) -> Decision {
        Decision {
            path: path.into(),
            resolution,
        }
    }

    /// One path, three states. `None` means absent.
    fn one(base: Option<char>, mine: Option<char>, theirs: Option<char>) -> SyncPlan {
        let build = |digit: Option<char>| match digit {
            Some(d) => manifest(&[("r.yml", d)]),
            None => Manifest::new(),
        };
        plan_sync(&build(base), &build(mine), &build(theirs))
    }

    // ----- the table of spec section 6 -----

    #[test]
    fn row_all_equal_does_nothing() {
        assert_eq!(one(Some('1'), Some('1'), Some('1')), SyncPlan::default());
    }

    #[test]
    fn row_only_theirs_changed_takes_theirs() {
        let plan = one(Some('1'), Some('1'), Some('2'));
        assert_eq!(plan.actions, vec![pull("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_only_mine_changed_keeps_mine_and_sends_it() {
        let plan = one(Some('1'), Some('2'), Some('1'));
        assert_eq!(plan.actions, vec![push("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_same_change_on_both_sides_does_nothing() {
        assert_eq!(one(Some('1'), Some('2'), Some('2')), SyncPlan::default());
    }

    #[test]
    fn row_different_changes_conflict() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::BothChanged,
                Some('2'),
                Some('3')
            )]
        );
    }

    #[test]
    fn row_deleted_here_and_untouched_there_takes_the_delete() {
        let plan = one(Some('1'), None, Some('1'));
        assert_eq!(plan.actions, vec![delete_theirs("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_deleted_here_and_edited_there_conflicts() {
        let plan = one(Some('1'), None, Some('2'));
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::DeleteVsEdit,
                None,
                Some('2')
            )]
        );
    }

    #[test]
    fn row_new_file_here_is_kept_and_sent() {
        let plan = one(None, Some('1'), None);
        assert_eq!(plan.actions, vec![push("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_new_file_there_is_taken() {
        let plan = one(None, None, Some('1'));
        assert_eq!(plan.actions, vec![pull("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_the_same_new_file_on_both_sides_does_nothing() {
        assert_eq!(one(None, Some('1'), Some('1')), SyncPlan::default());
    }

    #[test]
    fn row_two_different_new_files_conflict() {
        let plan = one(None, Some('1'), Some('2'));
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::BothAdded,
                Some('1'),
                Some('2')
            )]
        );
    }

    #[test]
    fn rows_with_the_roles_swapped_follow_the_same_rules() {
        let plan = one(Some('1'), Some('1'), None);
        assert_eq!(plan.actions, vec![delete_mine("r.yml")]);
        let plan = one(Some('1'), Some('2'), None);
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::DeleteVsEdit,
                Some('2'),
                None
            )]
        );
        // Deleted on both sides is the same change.
        assert_eq!(one(Some('1'), None, None), SyncPlan::default());
        assert_eq!(one(None, None, None), SyncPlan::default());
    }

    fn mirror(action: &SyncAction) -> SyncAction {
        match action {
            SyncAction::Pull { path } => push(path),
            SyncAction::Push { path } => pull(path),
            SyncAction::DeleteMine { path } => delete_theirs(path),
            SyncAction::DeleteTheirs { path } => delete_mine(path),
            SyncAction::RenameMine { from, to } => rename_theirs(from, to),
            SyncAction::RenameTheirs { from, to } => rename_mine(from, to),
            SyncAction::PullAs { .. } => panic!("the planner never emits PullAs"),
        }
    }

    #[test]
    fn swapping_the_two_sides_mirrors_the_plan() {
        let base = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '1'),
            ("c.yml", '1'),
            ("d.yml", '1'),
            ("e.yml", '1'),
            ("f.yml", '1'),
            ("g.yml", '1'),
        ]);
        let mine = manifest(&[
            ("a.yml", '2'),
            ("b.yml", '1'),
            ("c.yml", '2'),
            ("e.yml", '1'),
            ("f.yml", '2'),
            ("n.yml", '3'),
            ("x/moved.yml", '1'),
        ]);
        let theirs = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '2'),
            ("c.yml", '3'),
            ("d.yml", '1'),
            ("f.yml", '2'),
            ("m.yml", '4'),
        ]);
        let forward = plan_sync(&base, &mine, &theirs);
        let backward = plan_sync(&base, &theirs, &mine);
        let mirrored: Vec<SyncAction> = backward.actions.iter().map(mirror).collect();
        assert_eq!(forward.actions, mirrored);
        let swapped: Vec<Conflict> = backward
            .conflicts
            .iter()
            .map(|c| Conflict {
                path: c.path.clone(),
                kind: c.kind,
                mine: c.theirs.clone(),
                theirs: c.mine.clone(),
            })
            .collect();
        assert_eq!(forward.conflicts, swapped);
        assert!(!forward.actions.is_empty() && !forward.conflicts.is_empty());
    }

    #[test]
    fn a_plan_is_sorted_by_path() {
        let base = manifest(&[("b.yml", '1'), ("a.yml", '1')]);
        let mine = manifest(&[("b.yml", '1'), ("a.yml", '1'), ("c.yml", '2')]);
        let theirs = manifest(&[("b.yml", '2'), ("a.yml", '2'), ("0.yml", '3')]);
        let plan = plan_sync(&base, &mine, &theirs);
        assert_eq!(
            plan.actions,
            vec![pull("0.yml"), pull("a.yml"), pull("b.yml"), push("c.yml")]
        );
    }

    // ----- renames -----

    #[test]
    fn a_rename_by_the_peer_is_a_rename_here_not_a_delete_and_an_add() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &base, &manifest(&[("b.yml", '1')]));
        assert_eq!(plan.actions, vec![rename_mine("a.yml", "b.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn a_rename_here_tells_the_peer_to_move_its_file() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &manifest(&[("x/b.yml", '1')]), &base);
        assert_eq!(plan.actions, vec![rename_theirs("a.yml", "x/b.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn two_renames_of_equal_content_pair_in_path_order() {
        let base = manifest(&[("a.yml", '1'), ("b.yml", '1')]);
        let theirs = manifest(&[("d.yml", '1'), ("c.yml", '1')]);
        let plan = plan_sync(&base, &base, &theirs);
        assert_eq!(
            plan.actions,
            vec![rename_mine("a.yml", "c.yml"), rename_mine("b.yml", "d.yml")]
        );
    }

    #[test]
    fn a_renamed_and_edited_file_looks_like_a_delete_plus_an_add() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &base, &manifest(&[("b.yml", '2')]));
        assert_eq!(plan.actions, vec![delete_mine("a.yml"), pull("b.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn a_rename_by_the_peer_against_an_edit_here_is_a_conflict() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(
            &base,
            &manifest(&[("a.yml", '2')]),
            &manifest(&[("b.yml", '1')]),
        );
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "a.yml",
                ConflictKind::RenameVsEdit,
                Some('2'),
                Some('1')
            )]
        );
        // The rename stays in the plan as the marker that tells resolve the new path.
        assert_eq!(plan.actions, vec![rename_mine("a.yml", "b.yml")]);
    }

    #[test]
    fn a_rename_here_against_an_edit_by_the_peer_is_a_conflict() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(
            &base,
            &manifest(&[("b.yml", '1')]),
            &manifest(&[("a.yml", '2')]),
        );
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "a.yml",
                ConflictKind::RenameVsEdit,
                Some('1'),
                Some('2')
            )]
        );
        assert_eq!(plan.actions, vec![rename_theirs("a.yml", "b.yml")]);
    }

    #[test]
    fn a_rename_onto_a_path_the_other_side_holds_is_not_a_rename() {
        let base = manifest(&[("a.yml", '1')]);
        let mine = manifest(&[("a.yml", '1'), ("b.yml", '5')]);
        let theirs = manifest(&[("b.yml", '1')]);
        let plan = plan_sync(&base, &mine, &theirs);
        assert_eq!(plan.actions, vec![delete_mine("a.yml")]);
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "b.yml",
                ConflictKind::BothAdded,
                Some('5'),
                Some('1')
            )]
        );
    }

    #[test]
    fn a_file_renamed_on_both_sides_is_not_a_rename_either_way() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(
            &base,
            &manifest(&[("b.yml", '1')]),
            &manifest(&[("c.yml", '1')]),
        );
        assert_eq!(plan.actions, vec![push("b.yml"), pull("c.yml")]);
        assert!(plan.conflicts.is_empty());
        let same = plan_sync(
            &base,
            &manifest(&[("b.yml", '1')]),
            &manifest(&[("b.yml", '1')]),
        );
        assert_eq!(same, SyncPlan::default());
    }

    #[test]
    fn a_removed_file_and_an_added_file_with_other_content_are_not_a_rename() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &base, &manifest(&[("b.yml", '2')]));
        assert!(plan.actions.iter().all(|a| !matches!(
            a,
            SyncAction::RenameMine { .. } | SyncAction::RenameTheirs { .. }
        )));
    }

    // ----- resolve -----

    fn taken_of(mine: &Manifest, theirs: &Manifest) -> Manifest {
        let mut taken = Manifest::new();
        for source in [mine, theirs] {
            for path in source.paths() {
                taken
                    .insert(path, source.get(path).expect("listed path"))
                    .expect("valid entry");
            }
        }
        taken
    }

    #[test]
    fn resolve_refuses_a_missing_decision() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let err = resolve(&plan, &[], &Manifest::new()).expect_err("a conflict needs a decision");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn resolve_refuses_a_duplicate_or_unknown_decision() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let twice = [
            decide("r.yml", Resolution::KeepMine),
            decide("r.yml", Resolution::KeepTheirs),
        ];
        assert!(matches!(
            resolve(&plan, &twice, &Manifest::new()),
            Err(DomainError::InvalidInput(_))
        ));
        let stray = [
            decide("r.yml", Resolution::KeepMine),
            decide("other.yml", Resolution::KeepMine),
        ];
        assert!(matches!(
            resolve(&plan, &stray, &Manifest::new()),
            Err(DomainError::InvalidInput(_))
        ));
        // A plan without conflicts accepts no decisions at all.
        let clean = one(Some('1'), Some('1'), Some('2'));
        assert_eq!(
            resolve(&clean, &[], &Manifest::new()).expect("no conflicts"),
            vec![pull("r.yml")]
        );
        assert!(resolve(
            &clean,
            &[decide("r.yml", Resolution::KeepMine)],
            &Manifest::new()
        )
        .is_err());
    }

    #[test]
    fn keep_mine_and_keep_theirs_pick_a_side() {
        let both_changed = one(Some('1'), Some('2'), Some('3'));
        let taken = Manifest::new();
        assert_eq!(
            resolve(
                &both_changed,
                &[decide("r.yml", Resolution::KeepMine)],
                &taken
            )
            .expect("resolves"),
            vec![push("r.yml")]
        );
        assert_eq!(
            resolve(
                &both_changed,
                &[decide("r.yml", Resolution::KeepTheirs)],
                &taken
            )
            .expect("resolves"),
            vec![pull("r.yml")]
        );
        let delete_vs_edit = one(Some('1'), None, Some('2'));
        assert_eq!(
            resolve(
                &delete_vs_edit,
                &[decide("r.yml", Resolution::KeepMine)],
                &taken
            )
            .expect("resolves"),
            vec![delete_theirs("r.yml")]
        );
        assert_eq!(
            resolve(
                &delete_vs_edit,
                &[decide("r.yml", Resolution::KeepTheirs)],
                &taken
            )
            .expect("resolves"),
            vec![pull("r.yml")]
        );
        let edit_vs_delete = one(Some('1'), Some('2'), None);
        assert_eq!(
            resolve(
                &edit_vs_delete,
                &[decide("r.yml", Resolution::KeepMine)],
                &taken
            )
            .expect("resolves"),
            vec![push("r.yml")]
        );
        assert_eq!(
            resolve(
                &edit_vs_delete,
                &[decide("r.yml", Resolution::KeepTheirs)],
                &taken
            )
            .expect("resolves"),
            vec![delete_mine("r.yml")]
        );
    }

    #[test]
    fn keep_both_on_a_delete_against_an_edit_keeps_the_edit() {
        let taken = Manifest::new();
        let there = one(Some('1'), None, Some('2'));
        assert_eq!(
            resolve(&there, &[decide("r.yml", Resolution::KeepBoth)], &taken).expect("resolves"),
            vec![pull("r.yml")]
        );
        let here = one(Some('1'), Some('2'), None);
        assert_eq!(
            resolve(&here, &[decide("r.yml", Resolution::KeepBoth)], &taken).expect("resolves"),
            vec![push("r.yml")]
        );
    }

    #[test]
    fn keep_both_names_the_second_copy_from_peer() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let actions = resolve(
            &plan,
            &[decide("r.yml", Resolution::KeepBoth)],
            &manifest(&[("r.yml", '2')]),
        )
        .expect("resolves");
        assert_eq!(
            actions,
            vec![
                rename_theirs("r.yml", "r (from peer).yml"),
                SyncAction::PullAs {
                    path: "r.yml".into(),
                    as_path: "r (from peer).yml".into()
                },
                push("r.yml"),
            ]
        );
    }

    #[test]
    fn keep_both_counts_up_until_the_name_is_free() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let taken = manifest(&[
            ("r.yml", '2'),
            ("r (from peer).yml", '4'),
            ("r (from peer) 2.yml", '4'),
        ]);
        let actions =
            resolve(&plan, &[decide("r.yml", Resolution::KeepBoth)], &taken).expect("resolves");
        assert!(actions.contains(&SyncAction::PullAs {
            path: "r.yml".into(),
            as_path: "r (from peer) 3.yml".into()
        }));
    }

    #[test]
    fn keep_both_keeps_the_folder_and_handles_names_without_an_extension() {
        let base = manifest(&[("f/r.yml", '1'), ("f/folder", '1'), ("f/.hidden", '1')]);
        let mine = manifest(&[("f/r.yml", '2'), ("f/folder", '2'), ("f/.hidden", '2')]);
        let theirs = manifest(&[("f/r.yml", '3'), ("f/folder", '3'), ("f/.hidden", '3')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let decisions = [
            decide("f/.hidden", Resolution::KeepBoth),
            decide("f/folder", Resolution::KeepBoth),
            decide("f/r.yml", Resolution::KeepBoth),
        ];
        let actions = resolve(&plan, &decisions, &taken_of(&mine, &theirs)).expect("resolves");
        let names: Vec<&str> = actions
            .iter()
            .filter_map(|a| match a {
                SyncAction::PullAs { as_path, .. } => Some(as_path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                "f/.hidden (from peer)",
                "f/folder (from peer)",
                "f/r (from peer).yml"
            ]
        );
    }

    #[test]
    fn two_keep_both_conflicts_never_pick_the_same_name() {
        let base = manifest(&[("a.yml", '1'), ("a (from peer).yml", '1')]);
        let mine = manifest(&[("a.yml", '2'), ("a (from peer).yml", '2')]);
        let theirs = manifest(&[("a.yml", '3'), ("a (from peer).yml", '3')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let decisions = [
            decide("a (from peer).yml", Resolution::KeepBoth),
            decide("a.yml", Resolution::KeepBoth),
        ];
        let actions = resolve(&plan, &decisions, &taken_of(&mine, &theirs)).expect("resolves");
        let mut names: Vec<String> = actions
            .iter()
            .filter_map(|a| match a {
                SyncAction::PullAs { as_path, .. } => Some(as_path.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2);
        assert!(names
            .iter()
            .all(|n| n != "a.yml" && n != "a (from peer).yml"));
    }

    #[test]
    fn resolve_keeps_the_plain_actions_and_drops_the_rename_marker() {
        let base = manifest(&[("a.yml", '1'), ("c.yml", '1')]);
        let mine = manifest(&[("a.yml", '2'), ("c.yml", '1'), ("n.yml", '5')]);
        let theirs = manifest(&[("b.yml", '1'), ("c.yml", '2')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let actions = resolve(
            &plan,
            &[decide("a.yml", Resolution::KeepTheirs)],
            &taken_of(&mine, &theirs),
        )
        .expect("resolves");
        assert_eq!(
            actions,
            vec![
                pull("c.yml"),
                push("n.yml"),
                delete_mine("a.yml"),
                pull("b.yml")
            ]
        );
    }

    #[test]
    fn rename_against_edit_resolves_each_way() {
        let base = manifest(&[("a.yml", '1')]);
        let taken = Manifest::new();
        // The peer renamed, this side edited.
        let mine = manifest(&[("a.yml", '2')]);
        let theirs = manifest(&[("b.yml", '1')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let go = |r| resolve(&plan, &[decide("a.yml", r)], &taken).expect("resolves");
        assert_eq!(
            go(Resolution::KeepMine),
            vec![push("a.yml"), delete_theirs("b.yml")]
        );
        assert_eq!(
            go(Resolution::KeepTheirs),
            vec![delete_mine("a.yml"), pull("b.yml")]
        );
        assert_eq!(go(Resolution::KeepBoth), vec![push("a.yml"), pull("b.yml")]);
        // This side renamed, the peer edited.
        let plan = plan_sync(&base, &theirs, &mine);
        let go = |r| resolve(&plan, &[decide("a.yml", r)], &taken).expect("resolves");
        assert_eq!(
            go(Resolution::KeepMine),
            vec![delete_theirs("a.yml"), push("b.yml")]
        );
        assert_eq!(
            go(Resolution::KeepTheirs),
            vec![pull("a.yml"), delete_mine("b.yml")]
        );
        assert_eq!(go(Resolution::KeepBoth), vec![pull("a.yml"), push("b.yml")]);
    }

    #[test]
    fn resolve_refuses_a_rename_conflict_without_its_rename_marker() {
        let plan = SyncPlan {
            actions: Vec::new(),
            conflicts: vec![conflict(
                "a.yml",
                ConflictKind::RenameVsEdit,
                Some('2'),
                Some('1'),
            )],
        };
        let err = resolve(
            &plan,
            &[decide("a.yml", Resolution::KeepMine)],
            &Manifest::new(),
        )
        .expect_err("malformed plan");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    // ----- new_base -----

    /// Applies final actions to both sides the way the nodes do: renames, then deletes, then copies.
    fn simulate(
        mine: &Manifest,
        theirs: &Manifest,
        actions: &[SyncAction],
    ) -> (Manifest, Manifest) {
        let to_map = |m: &Manifest| -> BTreeMap<String, String> {
            m.paths()
                .map(|p| (p.to_string(), m.get(p).expect("listed path").to_string()))
                .collect()
        };
        let (mut a, mut b) = (to_map(mine), to_map(theirs));
        for action in actions {
            match action {
                SyncAction::RenameMine { from, to } => {
                    if let Some(hash) = a.remove(from) {
                        a.insert(to.clone(), hash);
                    }
                }
                SyncAction::RenameTheirs { from, to } => {
                    if let Some(hash) = b.remove(from) {
                        b.insert(to.clone(), hash);
                    }
                }
                _ => {}
            }
        }
        for action in actions {
            match action {
                SyncAction::DeleteMine { path } => {
                    a.remove(path);
                }
                SyncAction::DeleteTheirs { path } => {
                    b.remove(path);
                }
                _ => {}
            }
        }
        for action in actions {
            match action {
                SyncAction::Pull { path } => {
                    a.insert(
                        path.clone(),
                        theirs.get(path).expect("theirs has it").to_string(),
                    );
                }
                SyncAction::PullAs { path, as_path } => {
                    a.insert(
                        as_path.clone(),
                        theirs.get(path).expect("theirs has it").to_string(),
                    );
                }
                SyncAction::Push { path } => {
                    b.insert(
                        path.clone(),
                        mine.get(path).expect("mine has it").to_string(),
                    );
                }
                _ => {}
            }
        }
        (Manifest::from_entries(a), Manifest::from_entries(b))
    }

    fn assert_converges(
        base: &Manifest,
        mine: &Manifest,
        theirs: &Manifest,
        decisions: &[Decision],
    ) -> Manifest {
        let plan = plan_sync(base, mine, theirs);
        let actions = resolve(&plan, decisions, &taken_of(mine, theirs)).expect("resolves");
        let (after_mine, after_theirs) = simulate(mine, theirs, &actions);
        assert_eq!(
            after_mine, after_theirs,
            "both sides end equal for {actions:?}"
        );
        let stored = new_base(base, mine, theirs, &actions, decisions);
        assert_eq!(stored, after_mine, "the stored base is the agreed state");
        stored
    }

    #[test]
    fn new_base_is_the_agreed_state_for_a_clean_two_way_sync() {
        let base = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '1'),
            ("c.yml", '1'),
            ("d.yml", '1'),
            ("e.yml", '1'),
        ]);
        let mine = manifest(&[
            ("a.yml", '2'),
            ("b.yml", '1'),
            ("c.yml", '1'),
            ("e.yml", '1'),
            ("new-here.yml", '6'),
        ]);
        let theirs = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '3'),
            ("c.yml", '1'),
            ("d.yml", '1'),
            ("moved/e.yml", '1'),
            ("new-there.yml", '7'),
        ]);
        let stored = assert_converges(&base, &mine, &theirs, &[]);
        let expected = manifest(&[
            ("a.yml", '2'),
            ("b.yml", '3'),
            ("c.yml", '1'),
            ("moved/e.yml", '1'),
            ("new-here.yml", '6'),
            ("new-there.yml", '7'),
        ]);
        assert_eq!(stored, expected);
    }

    #[test]
    fn new_base_after_keep_mine_keep_theirs_and_keep_both() {
        let base = manifest(&[("r.yml", '1')]);
        let mine = manifest(&[("r.yml", '2')]);
        let theirs = manifest(&[("r.yml", '3')]);
        assert_eq!(
            assert_converges(
                &base,
                &mine,
                &theirs,
                &[decide("r.yml", Resolution::KeepMine)]
            ),
            manifest(&[("r.yml", '2')])
        );
        assert_eq!(
            assert_converges(
                &base,
                &mine,
                &theirs,
                &[decide("r.yml", Resolution::KeepTheirs)]
            ),
            manifest(&[("r.yml", '3')])
        );
        // Keep both: the original path takes mine and the second copy takes theirs.
        assert_eq!(
            assert_converges(
                &base,
                &mine,
                &theirs,
                &[decide("r.yml", Resolution::KeepBoth)]
            ),
            manifest(&[("r.yml", '2'), ("r (from peer).yml", '3')])
        );
    }

    #[test]
    fn new_base_drops_deleted_paths() {
        let base = manifest(&[("a.yml", '1'), ("b.yml", '1')]);
        let mine = manifest(&[("b.yml", '1')]);
        let theirs = manifest(&[("a.yml", '1')]);
        // a.yml was deleted here and b.yml there, both untouched on the other side.
        assert_eq!(
            assert_converges(&base, &mine, &theirs, &[]),
            Manifest::new()
        );
        // A delete against an edit follows the decision.
        let mine = manifest(&[("b.yml", '1')]);
        let theirs = manifest(&[("a.yml", '2'), ("b.yml", '1')]);
        let keep_edit = assert_converges(
            &base,
            &mine,
            &theirs,
            &[decide("a.yml", Resolution::KeepTheirs)],
        );
        assert_eq!(keep_edit, manifest(&[("a.yml", '2'), ("b.yml", '1')]));
        let keep_delete = assert_converges(
            &base,
            &mine,
            &theirs,
            &[decide("a.yml", Resolution::KeepMine)],
        );
        assert_eq!(keep_delete, manifest(&[("b.yml", '1')]));
    }

    #[test]
    fn every_resolution_of_every_conflict_kind_converges() {
        let base = manifest(&[("a.yml", '1')]);
        let scenarios: Vec<(Manifest, Manifest)> = vec![
            (manifest(&[("a.yml", '2')]), manifest(&[("a.yml", '3')])),
            (Manifest::new(), manifest(&[("a.yml", '3')])),
            (manifest(&[("a.yml", '2')]), Manifest::new()),
            (manifest(&[("a.yml", '2')]), manifest(&[("b.yml", '1')])),
            (manifest(&[("b.yml", '1')]), manifest(&[("a.yml", '3')])),
        ];
        for (mine, theirs) in &scenarios {
            for resolution in [
                Resolution::KeepMine,
                Resolution::KeepTheirs,
                Resolution::KeepBoth,
            ] {
                assert_converges(&base, mine, theirs, &[decide("a.yml", resolution)]);
            }
        }
        let added_base = Manifest::new();
        for resolution in [
            Resolution::KeepMine,
            Resolution::KeepTheirs,
            Resolution::KeepBoth,
        ] {
            assert_converges(
                &added_base,
                &manifest(&[("n.yml", '2')]),
                &manifest(&[("n.yml", '3')]),
                &[decide("n.yml", resolution)],
            );
        }
    }

    #[test]
    fn many_renames_with_long_shared_prefixes_pair_correctly() {
        let prefix = "folder/".repeat(100);
        let n = 5000usize;
        let mut base = Manifest::new();
        let mut theirs = Manifest::new();
        for i in 0..n {
            let hash = format!("{i:064x}");
            base.insert(&format!("{prefix}old{i:05}.yml"), &hash)
                .expect("valid entry");
            theirs
                .insert(&format!("{prefix}new{i:05}.yml"), &hash)
                .expect("valid entry");
        }
        let plan = plan_sync(&base, &base, &theirs);
        assert_eq!(plan.actions.len(), n);
        assert!(plan.conflicts.is_empty());
        assert_eq!(
            plan.actions[0],
            rename_mine(
                &format!("{prefix}old00000.yml"),
                &format!("{prefix}new00000.yml")
            )
        );
    }

    #[test]
    fn keep_both_name_never_collides_by_case() {
        let taken = manifest(&[("r.yml", '2'), ("R (From Peer).yml", '4')]);
        let plan = one(Some('1'), Some('2'), Some('3'));
        let actions =
            resolve(&plan, &[decide("r.yml", Resolution::KeepBoth)], &taken).expect("resolves");
        assert!(actions.contains(&SyncAction::PullAs {
            path: "r.yml".into(),
            as_path: "r (from peer) 2.yml".into()
        }));
    }
}
