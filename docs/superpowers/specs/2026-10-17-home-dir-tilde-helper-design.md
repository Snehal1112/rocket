# Single home-dir and tilde-expansion helper in rocket-shared

GitHub issue: https://github.com/Snehal1112/rocket/issues/54 (dedup T1.9, size M).

## Goal

Add one home-directory function and one tilde-expansion function to `rocket-shared`, and make every
place that resolves the home directory or expands `~` use them, so the rules are the same everywhere.

## Background

Rocket resolves the home directory in several ways today, found by a read-only survey of `main` at
`c94aad81`:

- Three hand-written copies read the environment:
  - `crates/rocket-infra/src/reqwest_executor.rs` (`read_certificate_file`, about line 986) expands only the
    prefix `~/`, using `HOME` then `USERPROFILE`. With no home it keeps the literal path, and the next check
    rejects it as not absolute.
  - `crates/rocket-app/src/grpc_service.rs` (`resolve_proto_path`, about line 541) expands only `~/`, using
    `HOME` then `USERPROFILE`. With no home it returns `InvalidInput("cannot find the home directory")`.
  - `crates/rocket-git/src/git2_service/helpers.rs` (SSH key callback, about line 171) tests only
    `starts_with('~')`, reads `HOME` only and replaces the first `~`. It expands a bare `~`, and it turns
    `~user/x` into `$HOME` plus `user/x`.
- Eight calls to `dirs::home_dir()`:
  - `crates/rocket-git/src/git2_service/ssh_host_verification.rs` (about line 35)
  - `crates/rocket-infra/src/aws_profile.rs` (about line 79)
  - `src-tauri/src/commands/git.rs` (about lines 885 and 903)
  - `src-tauri/src/commands/app.rs` (about lines 9 and 20)
  - `src-tauri/src/lib.rs` (about line 261)
  - `src-tauri/src/agent_session/scratch.rs` (about line 130, not named in the ticket)
- `crates/rocket-app/src/client_certificates.rs` `absolutize` keeps `~/` strings on purpose and does no home
  lookup.
- `rocket-shared` has no paths module and no `dirs` dependency. Every crate above can reach it. The
  workspace pins `dirs = "5"`, used today by `rocket-git`, `rocket-infra` and `src-tauri`.
- On Linux and macOS with `HOME` set, all copies give the same result. They differ when `HOME` is unset or
  empty, for `~` alone, for `~user`, and on Windows.

Line numbers are approximate and must be re-checked with grep before editing.

## Design

New file `crates/rocket-shared/src/paths.rs`, declared in `crates/rocket-shared/src/lib.rs`:

- `pub fn home_dir() -> Option<PathBuf>`: the one home lookup. It calls `dirs::home_dir()`.
- `pub fn expand_tilde(input: &str) -> PathBuf`: expands against `home_dir()`.
- `pub fn expand_tilde_with(input: &str, home: Option<&Path>) -> PathBuf`: the pure core that `expand_tilde`
  calls. Tests use it, so they never change the process environment.

Rules of `expand_tilde_with`:

| Input | Result |
|---|---|
| `~` | the home directory |
| `~/rest` | the home directory joined with `rest` |
| `~/` | the home directory |
| `~user`, `~user/rest` | unchanged |
| any other text, including the empty string | unchanged |
| any tilde form above that needs a home, when there is none | unchanged |

`rocket-shared` gets `dirs.workspace = true`. `rocket-git` and `rocket-infra` lose their `dirs`
dependency if nothing else in them uses it after the migration. `src-tauri` keeps `dirs` only if something
else there still needs it.

Call sites after the change:

- `reqwest_executor.rs`: `read_certificate_file` calls `expand_tilde`. The existing `is_absolute` check
  stays, so an unresolved tilde path is still rejected with the same error.
- `grpc_service.rs`: `resolve_proto_path` calls `expand_tilde` for tilde paths. When the input starts with
  `~` as an expandable form and no home exists, it still returns
  `InvalidInput("cannot find the home directory")`, with the same text.
- `helpers.rs`: the SSH key callback calls `expand_tilde`. A path that is not expandable is passed on
  unchanged, as today.
- The `dirs::home_dir()` calls listed above call `rocket_shared::paths::home_dir()`. Each keeps its own
  handling of a missing home (an `Option`, an empty list, or the existing error text).
- `client_certificates.rs` `absolutize` is not changed.

## Acceptance criteria

- `rocket-shared` has exactly one home-directory function (`home_dir`) and one tilde-expansion function
  (`expand_tilde`, with `expand_tilde_with` as its tested core).
- No `HOME` or `USERPROFILE` read and no `dirs::home_dir()` call remains outside `rocket-shared/src/paths.rs`
  in `crates/*/src` and `src-tauri/src` (production code), checked by a grep recorded in the final review.
- Unit tests of `expand_tilde_with` cover `~`, `~/x`, `~/`, `~user`, `~user/x`, the empty string, and an
  unset home for each tilde form.
- On Linux and macOS with `HOME` set, every migrated call site returns the same value as before. Tests that
  pin the old behavior of the three hand-written copies pass before and after the migration.
- `grpc_service` still reports `cannot find the home directory` when a `~/` proto path is given and there is
  no home.
- `cargo check -j4`, `cargo test -j4 -p rocket-shared paths`, `cargo test -j4 -p rocket-git` and
  `cargo test -j4 -p rocket-infra reqwest_executor` pass.

## Behavior that must not change

- On Linux and macOS with `HOME` set, `~/x` becomes `$HOME/x` everywhere it does today.
- The known-hosts path, the AWS credentials path and the `.rocket-api` folders resolve as the home directory
  joined with the same relative parts.
- `absolutize` keeps `~/` paths as written.
- Error types and messages at the call sites stay the same, except for the three differences below.
- No IPC contract, persistence format or serde attribute changes.

## Known behavior differences (accepted)

These follow from using one lookup and one grammar. Each is called out in the pull request text.

- A bare `~` now expands to the home directory in the certificate and proto-path code. Before, it was an
  error or a relative path there. The git SSH key code already expanded it.
- `~user/x` in an SSH key path is now left unchanged. Before, it was turned into `$HOME` plus `user/x`.
- With `HOME` unset or empty, the home directory now comes from the system account entry (the `dirs`
  behavior on Linux and macOS), and on Windows from the profile folder API. Before, the three hand-written
  copies failed or kept the literal path.

## Tests and measurement

- Unit tests in `crates/rocket-shared/src/paths.rs` use `expand_tilde_with` with explicit homes.
- One small test of `home_dir` checks that the result is absolute when it is `Some`. It does not set `HOME`.
- Before each migration, a test pins the current behavior of that call site when it can be tested without
  changing `HOME`. Where it cannot, the migration keeps the old branch structure and the unit tests of
  `expand_tilde_with` cover the new rules.

## Non-goals

- No change to `absolutize` and no new path handling for `~\` (backslash) forms.
- No new collection-name or workspace path builders. Those belong to tickets #55 and #56.
- No change to `canonicalize_within` (ticket #48).
- No new dependency other than moving `dirs` into `rocket-shared`.

## Decisions

The user ran `/factory` with the ticket link and said "details are in the ticket only", so no separate
interview answers exist. Each topic below uses the ticket as the answer. Items marked "my call" were not
decided by the ticket and are open to change when the user reviews this spec.

| Topic | Question asked | Answer |
|---|---|---|
| Purpose and who benefits | Why now? | From the ticket: one rule for home and tilde, and a latent Windows bug in the hand-written copies. |
| Constraints | What limits the change? | From the ticket: Linux and macOS behavior must not change, tests need a temporary `HOME` override or a pure function. Crates may depend only on `rocket-shared`, so the helper lives there. |
| Success checks | How is it measured? | From the ticket: one function each, no ad-hoc reads, tests for `~`, `~/x`, `~user` and an unset home, plus the four verification commands. |
| Edge cases and failure behavior | What if the home is unset? | The ticket asks for a test for an unset home. My call: unresolved tilde paths stay unchanged, and `grpc_service` keeps its explicit error. |
| Security and compatibility | What must not weaken? | From the ticket: no behavior, IPC or persistence change. My call: a relative or empty home is never accepted as a home, because `dirs` ignores an empty `HOME`. |
| Scope against related tickets | Overlap? | #55, #56 and #48 touch nearby path logic but not these files. My call: `scratch.rs` is included, because it has the same `dirs::home_dir()` pattern. |
| Non-goals | What is left out? | From the ticket: `absolutize` semantics stay. My call: no `~\` support, no `.rocket-api` path builders. |
| Alternatives rejected | What else was considered? | The ticket leaves open whether to use `dirs`. My call: use `dirs`, because three crates already depend on it and it handles the Windows profile folder. Rejected: hand-rolled `HOME` then `USERPROFILE` (no passwd or Windows API fallback, a third behavior to maintain). Also my call: bare `~` expands, instead of staying an error, so the grammar is the same everywhere. |
