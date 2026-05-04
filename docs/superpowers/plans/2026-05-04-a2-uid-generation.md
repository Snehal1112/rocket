# Fix A2 — UID generation: domain-owned policy, infra calls into it

**Branch:** `worktree-fix-a2-uid-generation`
**Source:** §2 / A2 of `.claude/reviews/2026-05-04-rocket-infra-synthesis.md`

## Problem (refined from synthesis)

The synthesis flagged two infra sites that mint UIDs by calling `uuid::Uuid::new_v4().to_string()` directly. On inspection, the picture is bigger and simpler than the synthesis suggested:

- The domain (`rocket-collection`) **already** has a UID-generation policy: a private `fn generate_uid() -> String` is duplicated in both `request.rs:7` and `folder.rs:4`. Both `Request::new`/`Folder::new` and the `#[serde(default = "generate_uid")]` attributes use it.
- `rocket-infra` mints UIDs in **seven** places, bypassing the domain's policy:
  - `oc_conversions.rs:933` — request deserialize fallback
  - `oc_conversions.rs:1160`, `:1279` — other conversion paths
  - `fs_collection_repo.rs:82` — UID fallback when reading legacy `.uid` sidecars
  - `fs_collection_repo.rs:223` — collection create
  - `fs_collection_repo.rs:418` — folder create
  - `fs_collection_repo.rs:548` — UID fallback during folder rebuild
  - `migration.rs:211` — UID generation for legacy entries

Each is a one-line `uuid::Uuid::new_v4().to_string()` call.

## Goal

Make `rocket-collection` the single owner of the UID-generation policy. Infra calls into it. No filesystem migration needed (existing on-disk YAML continues to deserialize via serde defaults exactly as today).

## Non-goals

- Do not touch `rocket-history` (`HistoryEntry::id` is a different concept).
- Do not touch `rocket-workspace` (`Workspace::id` is a different concept).
- Do not touch `src-tauri/.../oauth2.rs` (OAuth2 state nonce — unrelated).
- Do not touch `rocket-environment/src/dynamic_vars.rs` (the `{{guid}}` template helper — user-facing, deliberately stays a uuid call).
- Do not change UID format (still uuid v4 stringified).
- Do not add a constructor variant like `Request::with_uid(uid)` — out of scope.
- Do not remove the `uuid` dep from `rocket-infra` (still used for things outside A2).

## Design

### A2.1 — Promote `generate_uid` to public domain API

Create `crates/rocket-collection/src/uid.rs`:

```rust
/// The single source of truth for Request and Folder UID generation.
///
/// All persistence layers must call this rather than minting UUIDs directly,
/// so the format and policy are owned by the domain.
pub fn generate_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}
```

In `crates/rocket-collection/src/lib.rs`, add:

```rust
pub mod uid;
pub use uid::generate_uid;
```

Then in `request.rs` and `folder.rs`:

- Delete the private `fn generate_uid()` at `request.rs:7-9` and `folder.rs:4-6`.
- Replace each call site with `crate::generate_uid()` (or `crate::uid::generate_uid` — match the file's existing import style).
- The `#[serde(default = "...")]` attribute path must point at `crate::generate_uid` (or full module path) — pick whichever serde accepts and is consistent across both files.

### A2.2 — Rewire 7 infra sites

Replace direct mint with domain call in this exact set:

| File | Line | Current | New |
|---|---|---|---|
| `crates/rocket-infra/src/oc_conversions.rs` | 933 | `oc.uid.unwrap_or_else(\|\| uuid::Uuid::new_v4().to_string())` | `oc.uid.unwrap_or_else(rocket_collection::generate_uid)` |
| `crates/rocket-infra/src/oc_conversions.rs` | 1160 | `uid: uuid::Uuid::new_v4().to_string()` | `uid: rocket_collection::generate_uid()` |
| `crates/rocket-infra/src/oc_conversions.rs` | 1279 | `uid: uuid::Uuid::new_v4().to_string()` | `uid: rocket_collection::generate_uid()` |
| `crates/rocket-infra/src/fs_collection_repo.rs` | 82 | `uuid::Uuid::new_v4().to_string()` | `rocket_collection::generate_uid()` |
| `crates/rocket-infra/src/fs_collection_repo.rs` | 223 | `Some(uuid::Uuid::new_v4().to_string())` | `Some(rocket_collection::generate_uid())` |
| `crates/rocket-infra/src/fs_collection_repo.rs` | 418 | `Some(uuid::Uuid::new_v4().to_string())` | `Some(rocket_collection::generate_uid())` |
| `crates/rocket-infra/src/fs_collection_repo.rs` | 548 | `Some(uuid::Uuid::new_v4().to_string())` | `Some(rocket_collection::generate_uid())` |
| `crates/rocket-infra/src/migration.rs` | 211 | `uuid::Uuid::new_v4().to_string()` | `rocket_collection::generate_uid()` |

Line numbers are approximate — locate by the actual `uuid::Uuid::new_v4().to_string()` text and the surrounding context. After edits, no `uuid::Uuid::new_v4()` should remain in `rocket-infra` *except* in code paths that genuinely don't represent Request/Folder UIDs. (Check by grep before commit; if any remain, justify in the commit message or escalate.)

### A2.3 — Verify

```bash
cargo check -p rocket-collection
cargo check -p rocket-infra
cargo check                       # full workspace
cargo test -p rocket-collection
cargo test -p rocket-infra
```

All must pass. Specifically, `new_request_has_defaults` and `empty_folder` continue to pass — they construct via `Request::new`/`Folder::new` which now goes through the public `generate_uid`.

After commits, grep to confirm:

```bash
grep -n "uuid::Uuid::new_v4" crates/rocket-infra/src/
```

Should return only sites that were intentionally not in scope (none expected — A2.2 covers all 8 mint sites in infra; verify).

## Tasks

### A2.1 — Add `pub fn generate_uid()` to `rocket-collection`

**Files:** `crates/rocket-collection/src/uid.rs` (new), `crates/rocket-collection/src/lib.rs`, `crates/rocket-collection/src/request.rs`, `crates/rocket-collection/src/folder.rs`.

1. Create `uid.rs` with the public function as designed.
2. Add `pub mod uid;` and `pub use uid::generate_uid;` to `lib.rs`.
3. Delete the private `fn generate_uid()` from `request.rs` and `folder.rs`. Update each remaining call site (`Request::new`, `Folder::new`, `#[serde(default = "...")]`) to point at the public function via the path serde and Rust both accept — `crate::generate_uid` is preferred for in-crate calls.
4. Add a tiny test in `uid.rs`: `#[test] fn generate_uid_is_uuid_v4()` — assert the string parses as `uuid::Uuid` and that two calls return different values.

**Acceptance:** `cargo test -p rocket-collection` passes including the new test; no other file in the crate has its own `generate_uid` function.

### A2.2 — Replace 8 infra UID-mint sites

**Files:** `crates/rocket-infra/src/oc_conversions.rs`, `crates/rocket-infra/src/fs_collection_repo.rs`, `crates/rocket-infra/src/migration.rs`.

Apply the table above. Use `rocket_collection::generate_uid` (the function path); add a `use rocket_collection::generate_uid;` at the top of each file if it makes the call sites tidier — implementer's call.

After edits, run `grep -n "uuid::Uuid::new_v4" crates/rocket-infra/src/`. Expect zero matches that mint Request/Folder UIDs. If any survive, explain why in the commit message.

**Acceptance:** `cargo test -p rocket-infra` passes with no regressions; the grep above is clean.

### A2.3 — Verify and commit

After both prior tasks land:

```bash
cargo check
cargo test -p rocket-collection
cargo test -p rocket-infra
```

All must pass. Implementer should not need a separate commit for this task — it's the verification gate before handoff.

## Out-of-scope items to flag, not fix

- `rocket-history`, `rocket-workspace`, `src-tauri/oauth2.rs`, `dynamic_vars.rs` UUID calls.
- The synthesis's broader "UID minting on deserialize" framing — the `#[serde(default = "generate_uid")]` pattern is correct and stays.
- The `uuid` dependency listed in `rocket-infra/Cargo.toml` — only remove if grep proves nothing else in the crate uses it; do a final grep before deciding (likely still needed by `migration.rs` legacy paths or other tests).
