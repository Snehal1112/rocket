# Fix A5 — Move TauriTracingLayer out of rocket-infra

**Branch:** `worktree-fix-a5-tauri-tracing-move`
**Source:** §2 / A5 of `.claude/reviews/2026-05-04-rocket-infra-synthesis.md`

## Problem

`crates/rocket-infra/Cargo.toml:25` declares `tauri = { version = "2", features = [] }`. The dependency exists solely for `crates/rocket-infra/src/tauri_tracing_layer.rs`, which builds a `tracing_subscriber::Layer` that emits structured log events to the Tauri frontend via `AppHandle::emit`.

This violates the crate's stated role: rocket-infra provides filesystem and network implementations of domain repository traits. It is the only crate that does I/O *of that kind*. A `tracing` layer that depends on the Tauri delivery shell is not domain I/O — it is a desktop-shell concern that belongs alongside the other Tauri wiring in `src-tauri/`.

Today nothing else in `rocket-infra` requires the `tauri` crate. Removing the dep narrows the crate's surface back to its stated role and reduces compile-time coupling.

## Goal

- Move `tauri_tracing_layer.rs` (and its `BackendLogEntry`/`TauriTracingLayer` types) from `rocket-infra` into `src-tauri/src/tauri_tracing_layer.rs`.
- Drop the `tauri` dependency from `rocket-infra/Cargo.toml`.
- Update the four `rocket_infra::TauriTracingLayer` references in `src-tauri/src/lib.rs` to use the local module path (e.g. `crate::tauri_tracing_layer::TauriTracingLayer`).
- Preserve the existing test (`backend_log_entry_serializes_camel_case`) at the new location.

## Non-goals

- Do not change the layer's behavior, the event channel name (`"backend-log"`), or the `BackendLogEntry` schema (frontend depends on the camelCase serialization).
- Do not change visibility on `BackendLogEntry` — keep it `pub` so future code in `src-tauri` can reach it.
- Do not introduce a new sub-module like `src-tauri/src/tracing/`. Top-level placement matches the existing `src-tauri/src/audit_bridge.rs` style.
- Do not move the test to `rocket-shared` or anywhere else; it stays with the layer.

## Design

### File move

- Source: `crates/rocket-infra/src/tauri_tracing_layer.rs` (141 lines, contains `BackendLogEntry`, `TauriTracingLayer`, `FieldVisitor`, the `Layer<S>` impl, and one test).
- Destination: `src-tauri/src/tauri_tracing_layer.rs`.
- Move byte-for-byte. Do **not** edit the file's contents during the move; subsequent steps may adjust the test's `chrono` import path if needed, but the layer body is unchanged.

### `crates/rocket-infra/src/lib.rs` — remove the module

Today (`lib.rs:18` and `:34`):

```rust
mod tauri_tracing_layer;
...
pub use tauri_tracing_layer::{BackendLogEntry, TauriTracingLayer};
```

Delete both lines. Verify `cargo check -p rocket-infra` reports zero warnings about the removal (no other code in the crate references these types).

### `crates/rocket-infra/Cargo.toml` — drop the dep

Today (`Cargo.toml:25`):

```toml
tauri = { version = "2", features = [] }
```

Delete this single line. The surrounding lines (`tracing.workspace = true`, `tracing-subscriber.workspace = true`) are still needed by the rest of the crate.

### `src-tauri/src/lib.rs` — register the new module and rewrite the call sites

`src-tauri/src/lib.rs:1-3` currently declares:

```rust
mod audit_bridge;
mod commands;
mod tauri_event_bus;
```

Add `mod tauri_tracing_layer;` to that block, alphabetically:

```rust
mod audit_bridge;
mod commands;
mod tauri_event_bus;
mod tauri_tracing_layer;
```

Four references currently pull `TauriTracingLayer` from `rocket_infra`:

| Line | Current | New |
|---|---|---|
| 31 | `Option<rocket_infra::TauriTracingLayer>` | `Option<tauri_tracing_layer::TauriTracingLayer>` |
| 35 | `Option<rocket_infra::TauriTracingLayer>` | `Option<tauri_tracing_layer::TauriTracingLayer>` |
| 44 | `reload::Layer::new(None::<rocket_infra::TauriTracingLayer>)` | `reload::Layer::new(None::<tauri_tracing_layer::TauriTracingLayer>)` |
| 89 | `rocket_infra::TauriTracingLayer::new(app_handle.clone())` | `tauri_tracing_layer::TauriTracingLayer::new(app_handle.clone())` |

The `rocket_infra::` import block at lines 14-18 does not list `TauriTracingLayer` directly (current code uses the fully-qualified `rocket_infra::TauriTracingLayer`), so no import edits there are needed. Verify before editing.

The `eprintln!("Failed to activate TauriTracingLayer: {e}");` on line 91 is a string and does not need to change.

### Test preservation

The single test `backend_log_entry_serializes_camel_case` lives in `crates/rocket-infra/src/tauri_tracing_layer.rs:122-141`. After the move it must run as part of `cargo test -p rocket` (the `src-tauri` crate's package name from `src-tauri/Cargo.toml:2`). Verify it appears in the test output post-move.

## Tasks

### A5.1 — Move the file and rewire imports

**Files touched:**
- `crates/rocket-infra/src/tauri_tracing_layer.rs` — deleted.
- `crates/rocket-infra/src/lib.rs` — two lines removed.
- `crates/rocket-infra/Cargo.toml` — one line removed.
- `src-tauri/src/tauri_tracing_layer.rs` — created with the byte-identical contents of the deleted file.
- `src-tauri/src/lib.rs` — one `mod` line added, four `rocket_infra::TauriTracingLayer` references rewritten.

**Acceptance:**
- `cargo check -p rocket-infra` passes; `cargo check -p rocket` passes; `cargo check` (full workspace) passes.
- `grep -n "tauri" crates/rocket-infra/Cargo.toml` returns ZERO matches.
- `grep -rn "rocket_infra::TauriTracingLayer\|rocket_infra::BackendLogEntry" .` returns ZERO matches across the workspace (including `crates/` and `src-tauri/`).
- `cargo test -p rocket-infra` passes. The previously-existing `backend_log_entry_serializes_camel_case` test will no longer appear in this crate's output (expected).
- `cargo test -p rocket` passes; the test now appears in this crate's output.

### A5.2 — Verify

After A5.1 lands, run:

```bash
cargo check
cargo test -p rocket-infra
cargo test -p rocket
cargo test -p rocket-collection   # smoke check; should still pass
```

All must pass with no new warnings.

## Out-of-scope items to flag, not fix

- Renaming the `"backend-log"` event channel — frontend listens on this name.
- Refactoring `BackendLogEntry` into `rocket-shared` — it's only consumed by the layer; co-location is correct.
- Reorganizing `src-tauri/src/` into sub-modules — out of scope; matches existing flat layout.
