# Factory Spec and Plan Cards Plan Index

Created 2026-10-13. Spec: [2026-10-13-factory-spec-and-plan-cards-design.md](../../specs/2026-10-13-factory-spec-and-plan-cards-design.md).

Run the plans in order, one at a time. Each plan has at most 3 tasks. Each later plan builds on the one before it.

| Plan | File | Delivers | Status |
|---|---|---|---|
| 01 | [2026-10-13-factory-spec-cards-plan-01-driver.md](2026-10-13-factory-spec-cards-plan-01-driver.md) | `series.id`, one job per plan created up front (plans 2 to N parked at `spec`), `spec.md` copied into plan 1's folder, docs | pending |
| 02 | [2026-10-13-factory-spec-cards-plan-02-server.md](2026-10-13-factory-spec-cards-plan-02-server.md) | Server reads `series.id` and `cancelled`, builds spec cards, `/api/state` gets `specs[]` | pending |
| 03 | [2026-10-13-factory-spec-cards-plan-03-page.md](2026-10-13-factory-spec-cards-plan-03-page.md) | `spec S2` chip, slim waiting plan card, spec card, demo seed, docs | pending |

## Rulings

- Ruling: plans 2 to N of a series get their jobs when `factory new` runs, not after the plan before them merges — cost if wrong: job ids 002, 003 and so on are used up front and `factory status` lists plans that have not started.
- Ruling: `run_series` needs no new move logic, because `run_job` already moves a job at stage `spec` to `build` when its `plan.md` exists (`runloop.rs` `Stage::Spec` arm) — cost if wrong: none, the move is already tested by use.
- Ruling: a series made before this change has no `series.id`, so `run_series` keeps creating its next job after the previous one merges — cost if wrong: two code paths in `run_series` until old series are gone.
- Ruling: the spec title and goal are read from `spec.md` in plan 1's job folder, and only plan 1 gets a copy, so builder prompts for plans 2 and up stay as they are — cost if wrong: the spec card has no goal once plan 1's folder is cleaned.
- Ruling: the driver plan keeps `factory new` output unchanged (`created job <id> in <folder>`), because the `/factory` command parses it — cost if wrong: the user is not told how many waiting plans were created.
- Ruling: these plans are written for `superpowers:subagent-driven-development` or `superpowers:executing-plans`, not for the factory driver, because plan 01 changes the driver itself — cost if wrong: none.
