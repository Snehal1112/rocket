# Sandbox Developer Mode: Design

## Background

Rocket's JS scripting sandbox (`DenoScriptEngine`, `crates/rocket-infra/src/scripting/`) runs
pre-request/post-response/test scripts in a `deno_core` V8 isolate with a deliberately narrow,
hand-enumerated op table: no filesystem, network, or process access. This was the documented
output of a security audit (`docs/superpowers/plans/2026-09-16-scripting-security-roadmap.md`)
defending against a malicious script arriving via an imported collection.

The app's UI already has a "Safe Mode / Developer Mode" toggle (`SandboxPopover.tsx`,
`src/stores/sandbox-store.ts`) whose Developer Mode copy claims "Full filesystem and system
command access." An investigation (2026-09-21, via `decomposing-investigations`) found this is
entirely cosmetic: the store is a global `localStorage` value never sent over IPC, and the
backend wires exactly one engine unconditionally (`src-tauri/src/lib.rs:233`). The toggle changes
an icon and a warning banner and grants nothing.

Reference: Bruno (a comparable open-source API client) implements the same idea with two real
runtimes — a QuickJS/WASM "Safe Mode" (default, no Node access) and a Node.js `vm`-module
"Developer Mode" giving scripts genuine `fs`/`child_process`/`require()` access, scoped per
collection, opt-in via a Beta preference. This design follows that shape, adapted to Rocket's
`deno_core`-based engine (not Node.js, so no genuine `require()` of npm packages — see Non-goals).

## Goal

Make "Developer Mode" a real, per-collection setting that grants scripts genuine filesystem
read/write and process-execution capability, structurally absent (not just blocked) when the
collection is in Safe Mode, with an explicit confirmation gate before it can be turned on.

## Design

### 1. Data model

`CollectionSettings` (`crates/rocket-collection/src/settings.rs`) gains one field:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    #[default]
    Safe,
    Developer,
}
```

```rust
pub struct CollectionSettings {
    // ...existing fields (docs, auth, headers, variables)...

    /// JS sandbox capability level for scripts in this collection. Defaults to `Safe`
    /// (no filesystem/process access) so an imported collection never silently inherits
    /// an elevated capability from wherever it was authored.
    #[serde(default)]
    pub sandbox_mode: SandboxMode,
}
```

This persists to `collection.json` alongside `variables`/`auth`/`headers`, travels with the
collection (export/import/git), and defaults safely for any collection that predates this field
(`#[serde(default)]`).

### 2. Engine: structural gating, not runtime checks

`rocket_scripting_ext` (`crates/rocket-infra/src/scripting/engine.rs`) currently registers one
fixed op table. This design adds a second `deno_core::extension!`, `rocket_scripting_dev_ext`,
containing only the new `fs.*`/`process.*` ops (below). In `run_script()`, the extension list
passed to `RuntimeOptions` becomes conditional:

```rust
let mut extensions = vec![rocket_scripting_ext::init()];
if ctx.sandbox_mode == SandboxMode::Developer {
    extensions.push(rocket_scripting_dev_ext::init());
}
let mut runtime = JsRuntime::new(RuntimeOptions { extensions, ..Default::default() });
```

In Safe Mode the dev ops are never registered — `typeof fs === 'undefined'` in the isolate, not
"defined but throws." This matches the existing security posture and its regression tests
(`engine.rs:868-938`, "no internal globals reachable") rather than introducing a second,
weaker enforcement mechanism (a per-call runtime flag check inside every op, which only takes one
missed check to defeat). `bootstrap.js` feature-detects the ops' presence (`typeof
__ops.op_fs_read_file === 'function'`) to decide whether to define `globalThis.fs`/`globalThis.process`
at all, mirroring how it already builds every other global from the captured `__ops` closure.

### 3. New ops

All synchronous (`op2`, not async) — the engine never runs deno_core's event loop (confirmed by
the 2026-09-21 investigation: scripts are strictly synchronous, promises never resolve), and
Rust's blocking I/O calls fit that model directly, consistent with every existing op.

New files: `crates/rocket-infra/src/scripting/ops/fs.rs`, `crates/rocket-infra/src/scripting/ops/process.rs`.

| JS call | Op | Behavior |
|---|---|---|
| `fs.readFile(path, {encoding?})` | `op_fs_read_file` | `encoding` `"utf8"` (default) or `"base64"` |
| `fs.writeFile(path, content, {encoding?})` | `op_fs_write_file` | same encoding options |
| `fs.readDir(path)` | `op_fs_read_dir` | returns `[{name, isDirectory, isFile}]` |
| `fs.exists(path)` | `op_fs_exists` | boolean |
| `fs.mkdir(path, {recursive?})` | `op_fs_mkdir` | `recursive` defaults to `false` (matches Node's `mkdirSync`) |
| `fs.remove(path, {recursive?})` | `op_fs_remove` | file or directory; `recursive` defaults to `false` — removing a non-empty directory without it throws |
| `process.exec(command, args?, {cwd?, env?, timeoutMs?})` | `op_process_exec` | `{stdout, stderr, exitCode}`; `env` merges onto (does not replace) the app's own environment, matching Node's `child_process.exec` default |

No path restriction, no command allowlist/blocklist — matches Bruno's Node VM exactly, per
explicit user decision. All failures (not found, permission denied, non-zero considered success —
only spawn/IO failure throws) surface as a catchable JS `Error`, matching Node's synchronous `fs`
semantics.

`process.exec` invokes the binary directly (`std::process::Command::new(command).args(args)`) —
no shell string interpretation. This is not a capability restriction (any command is still
reachable); it avoids shell-quoting ambiguity, the same reasoning behind Node's
`execFile` existing alongside `exec`.

**`process.exec` needs its own timeout, independent of `SCRIPT_TIMEOUT`.** `SCRIPT_TIMEOUT`'s
enforcement mechanism is `v8::IsolateHandle::terminate_execution()`, which interrupts running JS
bytecode — it cannot interrupt a native Rust thread blocked inside `Command::output()`. Without a
dedicated timeout, a hung child process would block the script's OS thread past the outer 5s
budget with nothing to stop it. `op_process_exec` uses a poll-based wait with a deadline
(default `timeoutMs` = 5000, caller-overridable), killing the child and returning a timeout error
if exceeded. This is a resource-exhaustion safety net, not a capability restriction, consistent
with why `SCRIPT_TIMEOUT`/`SCRIPT_HEAP_LIMIT_BYTES` exist at all. The kill targets the direct
child process only, not any grandchildren it may have spawned — the same scope Node's
`child_process` defaults to without extra process-group flags; a script wanting guaranteed
subtree cleanup is a known, accepted limitation, not silently promised.

### 4. Wiring: settings → context → engine

`ScriptContext` (`crates/rocket-scripting/src/context.rs`) gains a `sandbox_mode: SandboxMode`
field (new enum owned by `rocket-scripting`, mirroring the existing `ExecutionMode` pattern —
`rocket-collection`'s `SandboxMode` and `rocket-scripting`'s `SandboxMode` are separate types by
design, keeping the two domain crates decoupled; `rocket-app` maps one to the other, the same
idiom already used for `execution_mode`). Defaults to `Safe` via the same `with_sandbox_mode(...)`
builder pattern `with_execution_mode(...)` already establishes, so every existing `ScriptContext`
constructor call site keeps compiling unchanged.

`RequestExecutionService` (`crates/rocket-app/src/execution_service.rs`) reads the active
collection's `CollectionSettings.sandbox_mode` once in `begin_phases` and threads it onto every
phase's `ScriptContext` — the same place `env_name` is already resolved once and reused across
`BeforeRequest`/`AfterResponse`/`Tests`. `CollectionRunnerService` needs no separate change since
it drives the same phase methods.

### 5. Frontend

`SandboxPopover.tsx` currently reads/writes `useSandboxStore`, a global value disconnected from
any collection. It's repointed at the active collection's settings:

- Reads `sandboxMode` from the currently-open collection's settings (same store/hook that already
  surfaces `variables`/`auth`/`headers` for the Collection Settings panel).
- No collection open → the popover shows a disabled state ("Open a collection to configure its
  sandbox mode") instead of Safe/Developer options.
- Selecting Developer Mode opens a shadcn `AlertDialog` (not a raw `confirm()` — this project's
  hard rule is shadcn primitives only) explaining the capability being granted and that it applies
  to this collection specifically ("especially risky for collections you didn't author yourself").
  Only on confirming does the setting actually save, via the existing collection-settings save IPC
  command (`save_settings`, already used for `variables`/`headers`/`auth` — no new command needed
  beyond adding the field to the existing DTO).
- Selecting Safe Mode saves immediately, no confirmation needed (only the escalation needs a gate).

`src/stores/sandbox-store.ts` (the global localStorage store) is deleted — it's fully superseded,
and keeping it around as dead code would be exactly the kind of unused-abstraction leftover this
project's conventions avoid.

## Non-goals

- **`require()` of local files or npm packages.** Bruno's Node VM mode includes a CommonJS module
  loader; Rocket's engine is `deno_core` (V8), not Node.js, and a genuine Node-compatible resolver
  is a materially separate effort. Explicitly out of scope per the 2026-09-21 brainstorming
  session — scripts get `fs`/`process` ops directly, no module loading.
- **Path or command restriction within Developer Mode.** Fully unrestricted once enabled, matching
  Bruno exactly, per explicit decision — this design does not add a sandboxed-subdirectory jail or
  a command allowlist.
- **Migrating existing collections' data.** `sandbox_mode` defaults to `Safe` via `serde(default)`;
  no migration script needed, no existing collection changes behavior.
- **Any change to the three existing script phases' semantics, the `rok`/`req`/`res`/`console` op
  tables, timeouts, or heap limits.** This design is additive (a new, separately-gated op table);
  nothing about the existing Safe Mode behavior changes.

## Interfaces

- `rocket_collection::settings::SandboxMode` (new, `Safe` default / `Developer`) — new field on
  `CollectionSettings`.
- `rocket_scripting::SandboxMode` (new, mirrors the above) — new field on `ScriptContext`, new
  `ScriptContext::with_sandbox_mode(...)` builder method.
- `rocket_infra::scripting::engine::rocket_scripting_dev_ext` (new `deno_core::extension!`) —
  conditionally registered in `run_script()`.
- `crates/rocket-infra/src/scripting/ops/fs.rs`, `.../ops/process.rs` (new files) — the 7 new ops.
- Frontend: `CollectionSettings` TS type gains `sandboxMode: 'safe' | 'developer'`; existing
  collection-settings save/load IPC path carries it, no new Tauri command.
- `src/stores/sandbox-store.ts` — deleted.
- `SandboxPopover.tsx` — rewritten to read/write the active collection's settings instead of the
  deleted global store; adds a shadcn `AlertDialog` confirmation on enabling Developer Mode.

## Testing

- Rust: unit tests per new op (`fs.rs`/`process.rs`), using `tempfile::TempDir` for fs roundtrips,
  matching this crate's existing test convention. `process.exec` tests split `#[cfg(unix)]`/
  `#[cfg(windows)]` where the invoked command differs, and include a timeout-exceeded case.
- Rust: a regression test proving Safe Mode still has `fs`/`process` structurally absent
  (`typeof fs === 'undefined'`), mirroring the existing "no internal globals reachable" tests at
  `engine.rs:868-938` — this is the test that would catch a future regression where dev ops leak
  into Safe Mode.
- Rust: an integration test with `sandbox_mode: Developer` exercising a real `fs.writeFile` +
  `fs.readFile` roundtrip and a real `process.exec` call end-to-end through `DenoScriptEngine`.
- Rust: `CollectionSettings` serde roundtrip test confirming `sandbox_mode` defaults to `Safe`
  when absent from an existing `collection.json` (backward compatibility).
- Frontend: `SandboxPopover` test confirming the confirmation dialog gates the Developer Mode
  save, and that Safe Mode saves without a dialog; a "no collection open" disabled-state test.

## Acceptance criteria

- A collection with `sandbox_mode: developer` in its settings runs scripts with working
  `fs.readFile/writeFile/readDir/exists/mkdir/remove` and `process.exec`.
- A collection without that field, or explicitly `safe`, has `typeof fs`/`typeof process` both
  `'undefined'` in every script phase — verified by a passing regression test, not just manual
  inspection.
- Enabling Developer Mode in the UI requires passing through the confirmation dialog; disabling
  it (back to Safe) does not.
- An imported collection with no prior `sandbox_mode` opens in Safe Mode.
- All three existing script phases (`BeforeRequest`/`AfterResponse`/`Tests`) and the Collection
  Runner all respect the setting identically — no phase-specific carve-out.
- Full workspace `cargo test --workspace -j 4` and `yarn tsc --noEmit`/`yarn check` pass.
