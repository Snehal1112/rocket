# Rust style rules

## Imports

- Groups separated by a blank line: `std` → external crates → internal crates → `crate::`.
- Match the existing ordering in the file you are editing.

## Errors

- Propagate with `?`. Do not `.unwrap()` / `.expect()` outside `#[cfg(test)]` code.
- All fallible service methods return `DomainResult<T>`.
- New error variants go on `DomainError` in `rocket-shared` when they are cross-cutting. Per-crate errors (e.g. `ContractError`) stay in their crate and convert at the boundary.
- Never stringify an error unless you have a concrete reason. Prefer `#[from]` / `#[error(transparent)]`.

## Cloning

- Prefer `Arc::clone(&x)` over `x.clone()` when cloning an `Arc` — it makes the intent explicit and matches the existing codebase.
- Avoid `.clone()` on large domain structs in hot paths. Borrow instead.

## Serde

- `#[serde(rename_all = "camelCase")]` on every struct that crosses the IPC boundary or persists to disk.
- New optional fields on persisted structs must use `#[serde(default, skip_serializing_if = "Option::is_none")]` so old saved files still deserialize.
- Enums on the wire use `#[serde(tag = "type", rename_all = "snake_case")]` — match the existing convention in `rocket-collection` and `rocket-shared`.

## Comments

- Short, full sentences ending with a period.
- Explain *why*, not *what*. If the code is self-evident, no comment.
- No emojis.
- Do not add doc comments to code that is already clear from its name and signature.

## Naming

- Services: `<Noun>Service` (e.g. `CollectionService`, `ContractService`).
- Repositories: `<Noun>Repository` (trait) and `Fs<Noun>Repo` (fs impl).
- Errors: `<Domain>Error` + `<Domain>Result<T>`.
- Domain IDs: `Ulid` for new features, `Uuid` where already established. Do not mix within one aggregate.

## Unsafe / panics

- No `unsafe` without a user discussion first.
- No `unimplemented!()` / `todo!()` in non-test code.
- `unwrap()` in tests is fine for brevity.
