# Testing rules

## Rust

- Tests live in `#[cfg(test)] mod tests` inside the module they test.
- Each service module has its own inline mock implementations of the traits it depends on. This keeps `rocket-app` tests free of any `rocket-infra` dependency.
- `tempfile` is only for `rocket-infra` and `rocket-workspace` tests that need a real filesystem.
- `wiremock` is only for `rocket-http` tests that need an HTTP mock.
- Tests must assert state transitions, not just that a function returned `Ok`. "Changed the method from GET to POST produces one changelog entry with field=method" beats "on_request_saved returned Ok".
- Cover the seams, not just the happy path: error propagation, edge cases, invariants. Example: a test that a failing repo call does not leak an orphan file.
- Run a single test with `cargo test -p <crate> <test_name>`.

## Frontend

- Vitest + React Testing Library.
- Test files live under `__tests__/` colocated with the file under test.
- Assert behaviour and state transitions, not render smoke. "Clicking Attach calls the store action with the right args" beats "the dialog renders a button".
- Mock `tauri-api.ts` at the boundary, not individual `invoke` calls.
- Run a single file with `yarn test --run <pattern>`.

## TDD workflow

- For non-trivial changes: write the failing test first, watch it fail, then implement.
- For mechanical changes (renames, formatting, clearly-correct extractions): implementation-first is fine. Use judgement.
- If a test is hard to write, the design is probably wrong. Fix the design, then write the test.

## Do not

- Do not test implementation details (private function internals, specific intermediate state). Test observable behaviour.
- Do not add tests for "future-proofing" scenarios that aren't in the spec.
- Do not mock the type system by casting to `any` / `unknown` — fix the types instead.
