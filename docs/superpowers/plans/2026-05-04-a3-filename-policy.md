# Fix A3 — Filename policy: domain-owned, drop empty-UID safety net

**Branch:** `worktree-fix-a3-filename-policy`
**Source:** §2 / A3 of `.claude/reviews/2026-05-04-rocket-infra-synthesis.md`

## Problem (refined from synthesis)

`fs_collection_repo.rs:303-372` (`save_request`) embeds three rules that aren't really infra concerns:

1. **Extension policy.** `.json` paths get rewritten to `.yml`; `.yml`/`.yaml` are left alone; bare paths get `.yml` appended. The on-disk extension is a domain choice (the OpenCollection spec stores requests as `.yml`).
2. **Collision-resolution naming.** When a request has no UID, the code picks the next free name as `"{stem} {n}.yml"` and walks `n` from 1 to a cap of `9999`. The format and the cap are user-facing decisions.
3. **Empty-UID safety net.** A 40-line loop using `OpenOptions::create_new(true)` to atomically claim a name, falling back through counter increments. This was a defense for a code path that, after A2, no longer exists — every path that constructs a `Request` mints a UID (via `Request::new` or serde default). Verified: importer (Bruno + Postman) uses `Request::new()`; frontend mints via `crypto.randomUUID()`/`tab.id`; serde defaults via `generate_uid`. There is no path in the workspace that produces an empty-UID `Request`.

## Goal

- Move the filename-format rules into `rocket-collection` as pure helpers.
- Replace the magic `9_999` with a domain constant.
- Replace the empty-UID safety net with a fail-fast `DomainError::Internal` so any future regression surfaces loudly instead of silently relying on the FS retry loop.
- Net effect: `save_request` becomes ~25 lines instead of ~70; collision-resolution code is deleted because it can no longer fire.

## Non-goals

- Do NOT change the `CollectionRepository` trait signature.
- Do NOT touch `get_request` (extension fallback handling is read-side and works).
- Do NOT change the on-disk format (still `.yml`).
- Do NOT introduce a `Request::with_uid` or other constructor — out of scope.
- Do NOT change the atomic write path (`atomic_write` continues to be used for the stable-UID path).
- Do NOT refactor `save_request_variables` or any other repo method.

## Risk note

Removing the empty-UID branch is the only meaningful behavior change. Mitigations:

- Verified all in-tree callers mint UIDs (Bruno importer, Postman importer, frontend, serde defaults).
- Failure mode is a `DomainError::Internal`, not a panic — bad caller surfaces loudly via the existing error pipeline; no app crash, no data corruption.
- Existing tests cover save with named UIDs; if any test was implicitly relying on the empty-UID rescue, it will fail at A3.2 verify time and we will surface it.

## Design

### A3.1 — Domain helpers

Add to `crates/rocket-collection/src/request.rs` (next to the `Request` impl):

```rust
/// Maximum number of `"{stem} {n}.yml"` candidates considered before giving up.
///
/// Purely a safety bound; current callers always pass a UID-bearing request, so
/// the collision loop is never taken in practice. Kept for any future migration
/// path that needs collision-aware naming.
pub const MAX_FILENAME_COLLISION_RETRIES: u32 = 9_999;

/// Apply the on-disk extension policy for a request file.
///
/// - `.yml` / `.yaml` → returned as-is.
/// - `.json` (legacy format) → rewritten to `.yml`.
/// - anything else → `.yml` appended.
///
/// The on-disk format is a domain choice (OpenCollection stores requests as
/// `.yml`); persistence layers must call this rather than embedding the rule.
pub fn request_filename_for(path: &str) -> String {
    if path.ends_with(".yml") || path.ends_with(".yaml") {
        path.to_string()
    } else if let Some(stem) = path.strip_suffix(".json") {
        format!("{stem}.yml")
    } else {
        format!("{path}.yml")
    }
}

/// Build a collision-disambiguating candidate filename `"{stem} {n}.yml"`.
///
/// `counter` is the disambiguation index (caller advances on `AlreadyExists`).
/// The format and `.yml` extension are domain rules; persistence layers must
/// call this rather than constructing their own filename strings.
pub fn candidate_filename(stem: &str, counter: u32) -> String {
    format!("{stem} {counter}.yml")
}
```

Tests in the same file's `#[cfg(test)] mod tests`:

- `request_filename_for_keeps_yml` — input `"foo.yml"` → `"foo.yml"`.
- `request_filename_for_keeps_yaml` — input `"foo.yaml"` → `"foo.yaml"`.
- `request_filename_for_migrates_json` — input `"foo.json"` → `"foo.yml"`.
- `request_filename_for_appends_when_missing` — input `"foo"` → `"foo.yml"`.
- `request_filename_for_handles_subpath_json` — input `"auth/login.json"` → `"auth/login.yml"`.
- `candidate_filename_format` — `("login", 1)` → `"login 1.yml"`.
- `candidate_filename_zero_counter_still_works` — `("login", 0)` → `"login 0.yml"` (purely a contract test; callers don't pass 0 today).

Re-export both helpers and the constant from `crates/rocket-collection/src/lib.rs`:

```rust
pub use request::{candidate_filename, request_filename_for, MAX_FILENAME_COLLISION_RETRIES, Request};
```

(Match the existing `pub use request::Request;` style.)

### A3.2 — Rewire `save_request` and remove empty-UID branch

In `crates/rocket-infra/src/fs_collection_repo.rs`, the `save_request` body becomes (approx):

```rust
fn save_request(&self, collection: &str, path: &str, request: &rocket_collection::Request) -> DomainResult<String> {
    if request.uid.is_empty() {
        return Err(DomainError::Internal(
            "save_request received Request with empty uid; callers must construct via Request::new()".into(),
        ));
    }

    let collection_dir = self.collection_path(collection);
    let normalized = rocket_collection::request_filename_for(path);
    let file_path = self.validate_path(&collection_dir, Path::new(&normalized))?;

    let oc = request_to_oc_http_request(request.clone());
    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize request YAML: {e}")))?;

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(&file_path, yaml.as_bytes())?;

    let actual = file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();
    Ok(actual)
}
```

(Implementer: keep the existing `#[tracing::instrument(...)]` attribute; preserve the trailing line that strips the prefix and returns the relative path, matching what the function returned before.)

Net deletions: the entire `else` block (lines ~334-373: the `loop` with `OpenOptions::create_new`, counter increment, 9999 cap, manual `write_all`, and `match e.kind()` arm). The `stem`, `parent_rel`, and `counter` locals go away with it.

The `9_999` constant and the `"{stem} {n}.yml"` format are no longer used in infra (the empty-UID branch that called them is gone). They live in `rocket-collection` from A3.1, available if a future migration ever reintroduces a collision path.

### A3.3 — Verify

```bash
cargo check -p rocket-collection
cargo check -p rocket-infra
cargo check -p rocket-import
cargo check
cargo test -p rocket-collection
cargo test -p rocket-infra
cargo test -p rocket-import
cargo test -p rocket-app
```

**Specifically watch:** any pre-existing `save_request` test in `fs_collection_repo.rs` that constructed a `Request` with an empty `uid` literal would fail. None should — all existing tests use `Request::new(...)` (which mints) per spot-check. If one fails, escalate; do not "fix" by re-introducing the empty-UID rescue.

`grep -n "9_999\|9999" crates/rocket-infra/src/` should return zero matches under the touched function (the constant has moved to domain).

## Tasks

### A3.1 — Add domain helpers

**File:** `crates/rocket-collection/src/request.rs` (and `lib.rs` for re-exports).

Add `request_filename_for`, `candidate_filename`, `MAX_FILENAME_COLLISION_RETRIES`, and the seven tests listed above. Re-export from `lib.rs`.

**Acceptance:** `cargo test -p rocket-collection` passes, including all seven new tests. Public API reachable as `rocket_collection::request_filename_for`, `rocket_collection::candidate_filename`, and `rocket_collection::MAX_FILENAME_COLLISION_RETRIES`.

**Out of scope:** any change to `crates/rocket-infra` or other crates.

### A3.2 — Rewire `save_request`

**File:** `crates/rocket-infra/src/fs_collection_repo.rs`.

1. Replace the body of `save_request` per the design above.
2. Delete the empty-UID collision loop entirely (the `else { loop { ... } }` block including the 9999 cap message).
3. Imports: add `use rocket_collection::request_filename_for;` (or fold into existing `rocket_collection` import group, matching A2's style).

**Acceptance:**
- `cargo test -p rocket-infra` passes — including pre-existing tests `request_save_overwrites_existing_uid_file`, `path_traversal_in_save_request_is_rejected`, etc.
- `grep -n "9_999\|9999" crates/rocket-infra/src/fs_collection_repo.rs` returns zero matches inside `save_request` (the comment about TOCTOU should be deleted along with the loop).
- `grep -n "create_new" crates/rocket-infra/src/fs_collection_repo.rs` returns zero matches.

**Out of scope:** any change to the `CollectionRepository` trait, `get_request`, `save_request_variables`, or other methods. Do not touch `crates/rocket-collection/`.

### A3.3 — Verify

After both prior tasks land, run the full verify command sequence above. No new warnings allowed.

## Out-of-scope items to flag, not fix

- The synthesis's wider point about "filename uniqueness policy in infra" is now fully addressed: format is in domain, magic-number is in domain, infra is purely the I/O caller.
- The unused `MAX_FILENAME_COLLISION_RETRIES` constant in domain is intentional — it's the public contract for any future migration path. Do not delete because it's unused at the moment.
