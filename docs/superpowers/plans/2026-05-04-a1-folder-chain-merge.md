# Fix A1 — Folder-chain variable merge: extract to domain

**Branch:** `worktree-fix-a1-folder-chain-merge`
**Source:** §2 / A1 of `.claude/reviews/2026-05-04-rocket-infra-synthesis.md`

## Problem

`fs_collection_repo.rs:617-659` (`get_folder_chain_variables`) walks each ancestor folder's `folder.yml`, applies a merge algorithm, and returns the result. The walk is correct infra responsibility. **The merge algorithm is a domain rule:**

- Outer-to-inner walk; inner wins on key collision.
- Disabled variables are skipped entirely — they do **not** shadow enabled vars from outer folders.
- Result is sorted by key.

This belongs in `rocket-collection` next to `CollectionVariable`. Today the rule cannot be unit-tested without `tempfile` + YAML fixtures, and any future caller has no way to reuse it.

## Non-goals

- Do **not** change the `CollectionRepository` trait signature.
- Do **not** touch the `unwrap_or_default()` data-loss bug at line 677 (that is S6, separate task).
- Do **not** rename or refactor `oc_variable_to_collection_variable`.
- Do **not** change call sites in `rocket-app` or `src-tauri`.

## Design

Add a pure function in `crates/rocket-collection/src/settings.rs`:

```rust
/// Merge a folder ancestor chain into a single deduplicated, sorted variable set.
///
/// `chain` is ordered outermost-first. For each folder, only enabled variables
/// participate; disabled entries are skipped entirely and do not shadow
/// enabled variables from outer folders. On key collision, the innermost
/// (later) folder wins. The returned vector is sorted by `key`.
pub fn merge_folder_chain_variables(
    chain: Vec<Vec<CollectionVariable>>,
) -> Vec<CollectionVariable> {
    use std::collections::HashMap;
    let mut merged: HashMap<String, CollectionVariable> = HashMap::new();
    for folder_vars in chain {
        for v in folder_vars {
            if v.enabled {
                merged.insert(v.key.clone(), v);
            }
        }
    }
    let mut out: Vec<CollectionVariable> = merged.into_values().collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}
```

Infra body becomes:

```rust
fn get_folder_chain_variables(
    &self,
    collection: &str,
    request_path: &str,
) -> DomainResult<Vec<CollectionVariable>> {
    let collection_dir = self.collection_path(collection);
    let path = std::path::Path::new(request_path);
    let dir_components: Vec<&str> = path
        .parent()
        .unwrap_or(std::path::Path::new(""))
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    let mut chain: Vec<Vec<CollectionVariable>> = Vec::new();
    let mut current = collection_dir.clone();
    for segment in &dir_components {
        current = current.join(segment);
        let folder_yml = current.join("folder.yml");
        if !folder_yml.exists() { continue; }
        let Ok(content) = fs::read_to_string(&folder_yml) else { continue; };
        let Ok(info) = serde_yaml::from_str::<OcFolderInfo>(&content) else { continue; };
        let Some(req) = info.request else { continue; };
        let Some(vars) = req.variables else { continue; };
        chain.push(
            vars.into_iter()
                .map(oc_variable_to_collection_variable)
                .collect(),
        );
    }
    Ok(rocket_collection::settings::merge_folder_chain_variables(chain))
}
```

Behavior is identical: same disabled-skip rule, same inner-wins rule, same sort.

## Tasks

### A1.1 — Add `merge_folder_chain_variables` to `rocket-collection`

**File:** `crates/rocket-collection/src/settings.rs`

Add the function exactly as designed above. Add unit tests in the same file's `#[cfg(test)] mod tests`:

1. `merge_empty_chain_returns_empty` — `merge_folder_chain_variables(vec![])` → `vec![]`.
2. `merge_single_folder_returns_sorted_enabled` — one folder with two enabled vars in reverse alpha order → returned sorted ascending.
3. `merge_inner_wins_on_collision` — outer `[k=outer]`, inner `[k=inner]` → result `[k=inner]`.
4. `merge_disabled_does_not_shadow` — outer `[k=outer enabled]`, inner `[k=inner_disabled disabled]` → result `[k=outer]`.
5. `merge_disabled_outer_not_present` — outer `[k=x disabled]` only → result `[]`.
6. `merge_three_levels_inner_wins` — `[a=1, b=1] / [a=2] / [b=3]` → `[a=2, b=3]`.

**Acceptance:** `cargo test -p rocket-collection` passes; new tests verify each rule explicitly. No I/O, no fixtures.

### A1.2 — Rewire `FsCollectionRepo`

**File:** `crates/rocket-infra/src/fs_collection_repo.rs`

1. Replace the body of `get_folder_chain_variables` (lines 617-659) with the design above. Keep the trait signature unchanged.
2. Add `use rocket_collection::settings::merge_folder_chain_variables;` at the top of the file (or call it via the path).
3. **Move three tests out of this file** into `rocket-collection/src/settings.rs`'s test module (already covered by tasks in A1.1; remove the infra duplicates):
   - `folder_chain_inner_wins_on_collision` (line 1431)
   - `folder_chain_root_request_returns_empty` (line 1476)
   - `folder_chain_skips_disabled_vars` (line 1487)
4. **Keep ONE integration test in infra** named `folder_chain_walks_disk_and_merges`. It writes two `folder.yml` files (outer with `k=outer`, inner with `k=inner`), calls `get_folder_chain_variables`, asserts inner wins. This proves the disk → domain wiring; it is **not** a re-test of the algorithm.

**Acceptance:** `cargo test -p rocket-infra` passes; the integration test proves end-to-end wiring; algorithm tests are gone from infra.

### A1.3 — Verify

Run, in this order:

```bash
cargo check -p rocket-collection
cargo check -p rocket-infra
cargo check  # full workspace — catches rocket-app / src-tauri compile breaks
cargo test -p rocket-collection
cargo test -p rocket-infra
```

All must pass with zero warnings introduced. Then commit with message:

```
refactor(collection): move folder-chain merge rule into domain

Extract the outer-to-inner / disabled-skip / sort-by-key merge from
FsCollectionRepo into rocket_collection::settings::merge_folder_chain_variables.
Infra retains the disk walk; the domain owns the rule and is unit-tested
without filesystem fixtures.

Addresses A1 of the rocket-infra DDD review (.claude/reviews/2026-05-04-rocket-infra-synthesis.md).
```

## Out-of-scope items to flag, not fix

- The four nested `if let` chains at lines 638-652 use `let-else` style after the rewrite; that's fine, do not also tackle the `unwrap_or_default` at :677.
- Do not change `oc_variable_to_collection_variable` even though it should be a `From` impl — that's listed separately under code-quality issues.
