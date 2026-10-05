# Harness

Hooks in `.claude/harness/hooks/` enforce these (details in `.claude/harness/README.md`):

- `cargo test|check|clippy|build` must pass `-j4`.
- No `cargo test --workspace` or `--all`.
- No bare `git stash` or `git stash pop`. Use `git stash push -u -m <tag>`, `apply`, `list`, `drop`.
- No `git add -A`, `--all` or `.`. Add explicit paths and commit with a pathspec.
- Before `git commit`, staged `.ts/.tsx` run `yarn tsc --noEmit` and `yarn check`, staged `.rs` run `cargo check -j4`. `HARNESS_GATE=0` skips.

## Orchestration convention

- One implementer per task, one at a time per worktree.
- A separate reviewer reviews a diff file, not the live tree.
- Re-reviews are scoped to the fixes since the last review.
- Keep a ledger at `.superpowers/sdd/<plan>/progress.md`. "Task N: complete" means done.
- Run a final whole-branch review before merge.
- Log decisions as `Ruling:` lines in the ledger.
- After `/clear` or compaction, use the `resume-plan` skill.
