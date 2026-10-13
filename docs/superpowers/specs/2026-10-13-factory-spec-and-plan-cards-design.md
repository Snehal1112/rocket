# Factory dashboard: spec card and waiting plan cards

Date: 2026-10-13. Status: design, approved in conversation, spec awaiting review.
Builds on: `.claude/software-factory.md` (v3 Rust driver) and the dashboard in `tools/factory-server/`.

## Problem

The Spec column of the factory dashboard is always empty. `factory new` creates a job already at stage `build`, so no card ever sits in `spec`. A plan that has not started has no job, so it has no card. `run_series` creates the job for plan k+1 only after plan k merges. The spec of a series is copied into plan 1's job folder only, and nothing links the plan jobs of one series together.

## Goal

1. One spec card per series (one spec, several plans), showing the spec title, its goal and one row per plan with its state.
2. One card per plan, existing from the start. Plans that have not started wait in the Spec column with all their tasks shown as waiting, and move to Build when their execution starts.
3. Each plan card shows which spec it belongs to.

Success: for one spec with 5 plans of 3 tasks, the Spec column shows the spec card and plan cards 2 to 5 while plan 1 builds. Plan cards move one by one as the series runs.

## Non-goals

- No change to how plans are written, validated or merged. The 3-task and 6-plan limits stay.
- No new token cost. Parked jobs never start an agent, and builder prompts for plans 2 and up stay as they are today.
- No spec or plan files are written to `docs/superpowers` by the factory.
- Series that already ran (jobs 001 to 004) are not rewritten.

## Design

### Driver (`tools/factory`)

- **Series id.** `factory new --plans <path>` allocates a short series id `S<n>`, where `n` is one more than the highest number already used by any job's series on the board (starting at 1). The id goes into `SeriesPos` as an optional `id` field. Jobs without it are old jobs.
- **Jobs up front.** `create_job` parses the series once and creates one job per plan, with sequential ids. Plan 1 starts at stage `build` as today. Plans 2 to N are created at stage `spec`. Each job gets its own folder with `progress.md` and its own `plan.md` copied from its plan file, and the same `series { id, plan_file, index, total }`.
- **Spec file.** If the `--plans` path is a folder with a `spec.md`, it is copied into plan 1's job folder. Later jobs do not get a spec copy, so the builder prompt line "The spec is at ..." stays plan-1-only, as it is today.
- **Series with one plan.** No series id, no extra jobs, no spec card. Behaviour is unchanged.
- **`run_series`.** The existing lookup by `plan_file` and `index` finds the parked job and moves it to `build` instead of creating one. `factory run <id>` on a parked job also works: the `Spec` arm in `run_job_inner` already moves a job with a `plan.md` to `build`.
- **Cancel.** `factory cancel <id>` on a plan job sets that job's `cancelled` flag only. A cancelled plan stops `run_series`, so the later parked jobs stay at `spec` until the user cancels them or reruns. The spec card shows the cancelled plan as `cancelled`.
- **`factory status`.** Lists parked jobs with stage `spec`. No change to its code beyond the new rows.
- **Compatibility.** `id` is optional and defaulted, so old boards load and old jobs behave as before. The board stays valid for an older dashboard server.

### Server (`tools/factory-server`)

- **Model.** `Series` gains the series id (`id` already exists as a loose value and is tightened to an optional string).
- **Grouping.** `build_state` groups jobs by series id. For each group with `total >= 2` it adds a spec card to `/api/state`: `{series_id, title, goal, plans: [{job_id, index, title, state}], stage}`.
- **Spec title and goal.** Read from `spec.md` in the folder of the job with `index == 1`, through the existing safe folder-name check. The title is the first `# ` line and the goal is the first paragraph after `## Goal`, using the existing `first_heading` and `goal` helpers. A missing `spec.md` gives the series id as the title and no goal.
- **Cancelled flag.** The server `Job` model gains an optional `cancelled` bool, which the driver already writes (`factory-core/src/board.rs:87-89`).
- **Plan row state.** `cancelled` if the flag is set, otherwise from the job's stage: `spec` is waiting, `build` is building, `review` and `approve` are in review, `needs-human` is needs human, `merged` is merged.
- **Spec card stage.** `merged` once all plans are merged. `needs-human` if any plan is cancelled or needs a human. Otherwise `spec`. Old jobs without a series id produce no spec card.
- **Plan card data.** Each job already carries its tasks through `load_plan`. A parked job has stage `spec`, which the existing stage rule leaves as all tasks pending, so no change is needed.

### Page (`tools/factory-server/static`)

- **Plan card.** The existing card, with one new chip `spec S2` in the header next to `plan k of N`, with the spec title as the hover label. A job at stage `spec` hides the parts that are empty before a start: stage times, token bar, agent activity and the branch and files box. The task list stays open by default, as it is for any job that is not merged.
- **Spec card.** A new `renderSpecCard`, same card shell and styles. Header: `S2 SPEC [5 plans]`. Then the title, the goal, a segment bar with one segment per plan, and a collapsible "Plans n/N done" list. Its rows reuse the `.task` row styles: a dot, `Plan k <title>` and the state label. It has no tokens, agents or branch box.
- **Columns.** The spec card is placed by its stage like any card.
- **Demo.** `tests/seed-demo.sh` gets one 5-plan series, so the look can be checked in a browser.

## Error handling

- A bad or missing series id on a job: the job renders as an old job, with no chip and no spec card.
- A group whose plan-1 job is missing (for example a cleaned job): the spec card still appears, with the series id as its title.
- Unknown plan state or stage: shown as waiting, like the page's existing fallbacks.

## Testing

- Driver: series id allocation, one job per plan with correct stages, folder contents (`plan.md` for each, `spec.md` for plan 1 only), `run_series` reusing parked jobs, single plan unchanged.
- Server: grouping by series id, spec title and goal reading, plan row states, old boards without an id.
- Page: node tests for the spec card, the `spec S2` chip, and the hidden empty sections of a parked card, including HTML escaping of the titles.
- Manual: run the seed-demo script and look at dark, light and phone width in a real window.
- Checks per `.claude/rules/harness.md`: every cargo call with `-j4`, no `--workspace`.

## Open items

None. The "Plans" list reads from plan jobs, so plan titles come from each plan's first heading as the cards already do.
