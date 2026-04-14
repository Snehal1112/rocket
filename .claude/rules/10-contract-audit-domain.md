# Contract audit domain rules

Read this before touching any contract snapshot, changelog, or `on_request_saved` logic.

## What "contract audit" means

A contract is attached to a collection scope (collection, folder, or request).
Every `save_request` call triggers `on_request_saved`, which:

1. Finds all contracts whose scope `covers()` the saved request's relative path.
2. Loads the stored `RequestSignatureSnapshot` for that request.
3. Diffs old vs new via `diff_signature` — returns `Vec<FieldChange>`, empty = no change.
4. If changes exist, appends a `ChangelogEntry` to the contract's `ContractChangelog`.
5. Upserts the new snapshot as the baseline for future diffs.

The baseline is only meaningful if it was populated when the contract was first attached. An empty baseline means step 3 always produces an empty diff and the changelog is never written.

## `covers()` semantics (ContractScope)

- `Collection` — covers every request in the collection.
- `Folder { rel_path }` — covers requests whose path starts with `rel_path`.
- `Request { rel_path }` — covers only that exact path.

## Snapshot storage layout

```
<collection-root>/.rocket/contracts/
  <id>.yml             — contract definition
  <id>-snapshot.yml    — ContractSnapshot (one per contract)
  <id>-changelog.yml   — ContractChangelog (one per contract)
```

`list_contracts` in `FsContractRepo` filters out `-snapshot` and `-changelog` files.

## Key invariants

- **`on_request_saved` must never fail the caller.** It is called with `let _ = ...` inside `save_request`. Errors are `log::warn!`-ed. Return type is `()`. Do not change this.
- **`diff_signature` is correct as reviewed.** It uses `contains()` on slices so field-order changes do not produce spurious entries. Do not touch it.
- **`Warn` / `Block` enforcement modes are no-ops.** Leave them as `log::warn!` stubs — a later sprint implements them.

## Test mock pattern

Tests in `rocket-app` use inline mocks implementing `ContractRepository`. The mock holds `Vec<Contract>`, a `RefCell<HashMap<Ulid, ContractSnapshot>>`, and a `RefCell<HashMap<Ulid, ContractChangelog>>`. See existing tests in `rocket-app` for the exact struct shape — do not re-derive it from scratch.

For C1 tests the `CollectionRepository` mock must return a `Vec<Request>` that matches the contract scope.

## What NOT to do

- Do not read from `rocket-infra` in tests. Mock the trait.
- Do not change the IPC wire type (`initialSnapshots` field in `tauri-api.ts` stays — backend ignores it).
- Do not move `ContractRepository` to `rocket-infra`. It is a domain trait and belongs in `rocket-collection`.
