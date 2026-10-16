# Factory Cost Reduction Plans

Spec: `docs/superpowers/specs/2026-10-17-factory-cost-reduction-design.md`. Run the plans in order, one at a time.

| Plan | File | Tasks | Delivers |
|---|---|---|---|
| 1 | `plan-01-commit-batch.md` | 3 | Config switches, board field, git resets, batch committer kind, snapshot commits, one committer per job |
| 2 | `plan-02-builder-excerpt.md` | 2 | Task-only builder prompts and the builder agent file change |

Plan 2 uses `Config.builder_excerpt`, which plan 1 adds.
