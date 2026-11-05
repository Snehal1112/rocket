# Phase 5 audit fixes: plan index

Source: `.claude/reviews/2026-11-04-ddd-phase5-audit.md`. Each plan has 3 tasks or fewer. Run them in order, one at a time.

| # | Plan | Closes | Tasks | Crates |
|---|---|---|---|---|
| 1 | [Import symlink hardening](2026-11-04-phase5-fixes-plan-1-import-symlink-hardening.md) | S-1, S-2, C-1, C-5, finishes D-51 | 3 | `rocket-import` |
| 2 | [Import limits](2026-11-04-phase5-fixes-plan-2-import-limits.md) | S-4, S-5 | 2 | `rocket-import` |
| 3 | [Token client hardening](2026-11-04-phase5-fixes-plan-3-token-client-hardening.md) | S-3, S-6, T-1, T-2, C-7 | 2 | `rocket-infra` |
| 4 | [Dependency cleanup](2026-11-04-phase5-fixes-plan-4-dependency-cleanup.md) | C-2, C-3, C-4 | 3 | `rocket-import`, `rocket-http`, `rocket-git`, `src-tauri` |
| 5 | [Docs and tests](2026-11-04-phase5-fixes-plan-5-docs-and-tests.md) | C-6, C-8, C-9, C-10 | 3 | docs, `rocket-infra`, `rocket-http` |

## Order and conflicts

- Plans 1 and 2 both edit `rocket-import/src/importer.rs`; run 1 first.
- Plans 3 and 4 touch different files from 1 and 2 and could run in parallel worktrees, but the repo rule is one implementer per worktree, so run them in sequence.
- Plan 5 goes last because its docs describe the result of 1 to 4.
- Not covered on purpose: S-7 (a Postman environment with an existing name is replaced silently) and T-3, T-5, T-6. S-7 needs a product decision (overwrite, rename or ask).

## Verification for the whole series

`cargo check -j4`, `cargo clippy -j4 -p rocket-import -p rocket-http -p rocket-infra -p rocket-git -- -D warnings`, `cargo test -j4 -p <crate>` for each touched crate, `cargo fmt --check`.
