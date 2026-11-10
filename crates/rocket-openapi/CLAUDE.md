# CLAUDE.md

Guidance for Claude Code when working in the `rocket-openapi` crate.

## What This Is

The `rocket-openapi` crate is the home of the OpenAPI sync context: the sync
service (`OpenApiSyncService`), apply, events and view DTOs. The background
scheduler loop stays in `rocket-app` (`openapi_sync_scheduler.rs`) and calls
this service. Feature docs: `docs/features/openapi-sync-ux.md`.

## Dependency Direction

- May use `rocket-shared`, `rocket-collection`, `rocket-import`, `rocket-http`
  and `rocket-environment`. Other deps: `serde`, `serde_json`, `serde_yaml`,
  `chrono`, `sha2`, `tracing`, `async-trait` and `zeroize` (token text).
- Never depends on `rocket-app`. `rocket-app` and `src-tauri` depend on this
  crate. `rocket-infra` is a dev-dependency only (tests).
- The collection lock comes from `rocket_collection::collection_lock`.

## Commands

```bash
# Check this crate
cargo check -p rocket-openapi -j4

# Run all tests in this crate
cargo test -p rocket-openapi -j4
```

## Module Map

Most modules have a `*_tests.rs` file beside them, mounted through the `#[path]`
chain in `apply_tests.rs`; `service`, `events` and `view` keep their tests inline.
Some test files cover a topic: `consent_tests.rs`, `disconnect_tests.rs`,
`file_source_tests.rs`, `plan_tests.rs`.

| Module | Responsibility |
|---|---|
| `service` | `OpenApiSyncService` and its builder (`with_apply`, `with_lock_keys`, `with_local`, `with_consent`, `with_backups`), `check`, `due_targets`, `status`, consent, `set_interval`, `set_token`; `CheckOutcome`, `SyncStatusView`, `SyncTarget`, `safe_message` |
| `apply` | `apply(collection, &Selection)`, `recover`, `ApplyDeps`, `RendererFactory` |
| `changes` | The changes view and overview of a pending sync (local files only) |
| `skips` | `SkippedRow`, the rows the apply left out and why |
| `delete` | `DeleteTarget`, delete of a synced request or folder |
| `reset` | `reset`, `restore`, `discard`, `ResetReport` |
| `spec_view` | `spec_view`, the View spec text (local files only) |
| `spec_source` | The spec source file and its local state |
| `connect` | `preview`, `confirm`, `request_digest` and the connect row types |
| `change_link` | Change the link of a connected collection (`ChangeLinkPreview`, `ChangeLinkOutcome`) |
| `events` | `OpenApiSyncEvent` (`OpenApiSyncChecked`, `OpenApiSyncApplied`), the refresh signals the sync publishes, with wire contract tests |
| `view` | Read-only views of a pending sync (`PlanView`, `SpecView`, `ChangesView`, ...). No serde; the IPC layer maps them to its own DTOs |

`OpenApiSyncService::lock_of` and `service::sha256_hex` are `pub` and
`#[doc(hidden)]` only because `rocket-app` tests use them. Production code
outside this crate must not call them.

## Service Rules

- **Local state.** The service needs a local repository (`with_local`, or the
  one in the apply deps). Without one, `check`, `set_interval` and `status`
  return an error. Timing lives in `OpenApiLocalState`; `due_targets` reads
  timing only from there through `rocket_collection::next_check_at`, never from
  the legacy fields of the source file. It needs apply deps and is empty
  without them.
- **Consent gate.** The opt-in is machine-local consent
  (`rocket_collection::OpenApiConsentStore`, file
  `~/.rocket-api/openapi-sync.yml`, keyed by canonical root and `source_id`,
  holding the structural `location_key` of the approved location).
  `due_targets` skips a collection without consent for its current location.
  `status()` reports `consent` (`granted`, `missing`, `locationChanged`) and
  shows interval off and no token read unless it is granted. `check` returns
  `Failed { code: "consent_required" | "location_changed" }` before any
  recovery, keychain read or request, and gates twice, before and after
  `settle` (journal recovery can rewrite the source file). It fetches with the
  source object that passed the second gate. The keychain read is gated too.
- **Granting.** Only `grant_consent(root, expected_location)` grants. Under the
  collection lock it reloads the source, requires the shown text to match and
  refuses a location that does not survive `parse_spec_url(display_url)`.
  `grant_consent_for_link` is the import path. `set_interval` (not Off) and
  `set_token` need `Granted` before any write, including the keychain write.
  `set_interval(Off)` always works. A service without `with_consent` treats
  every source as unapproved.
- **Location key.** The record holds a structural key (`location_key`: the
  `SpecLocation` variant and every field as JSON), never the display text,
  because different locations can display alike. An old record without the key
  reads as missing. `opt_in_root` stays readable but is ignored and no longer
  written.
- **Token.** The keychain token is keyed by `source_id` only, so approving a
  cloned folder that reuses a `source_id` uses that token against the clone's
  own GitHub location. Logs carry codes and counts only.
- **Failures and timing.** A failed check adds one to `consecutive_failures` and
  sets `not_before` from `Retry-After`; a finished one resets both. `apply`
  keeps the interval and timing it loaded.
- **Remote refs.** `check` follows the files a spec refers to by URL (remote
  `$ref`). A main-spec 304 is confirmed file by file (`refs_unchanged_labeled`)
  when refs are stored, and skipped for a full fetch when
  `remote_refs_incomplete` is set. A failure on that path keeps its usual code
  with a fixed message naming the file. A fresh body is bundled by
  `bundle_spec`, and the bundled text is what is hashed and kept as the pending
  copy. A bundle that lost a file stored last time (`incomplete` and a stored
  URL missing from `Bundle.refs`) fails the check with code `ref_unavailable`
  and leaves the source, hash, pending copy and stored refs as they were. A
  file that never loaded does not fail the check. See
  `crates/rocket-import/CLAUDE.md` (Remote references).
- **Reset, restore, discard.** One file transaction each, under the collection
  lock. Reset and View spec read local files only. Overview, changes and badges
  never touch the network or the keychain.
- **Connect and confirm.** `preview` fetches the spec and returns a request
  digest. `confirm` fetches the spec once more, checks the digest and the spec
  hash against the preview, commits the files in one transaction, then
  approves the typed link, sets the interval and stores the token. Later
  failures are warnings. Errors cross IPC as fixed sentences.
- **Lock order.** The collection lock (`rocket_collection::collection_lock`)
  comes first, then the file transaction's own guard. `apply`, `recover`,
  `set_interval` and `set_token` hold the lock. `check` takes it only after its
  fetch, reloads the source and writes just its own fields. The sync apply
  never calls `CollectionService`.
