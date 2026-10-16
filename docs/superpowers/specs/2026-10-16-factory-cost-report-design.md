# Factory cost report and estimate (sub-project C)

Part of a three-part factory improvement: A accurate usage (built), B failure details on `needs-human` cards (built), C cost report and estimate by job size. C reads the per-run records A writes. It continues on the branch `factory-accurate-usage` and merges with A and B.

## Goal

Show the measured token cost of finished factory jobs grouped by plan size, estimate the cost of a new job from that history, and make the split between roles visible, so cost can be understood and then reduced.

## Background

- Jobs 003 to 011 have `usage.jsonl` (jobs 001 and 002 have none). Each job is one plan of two or three tasks, so the task count barely separates small from complex.
- Weighted cost per job is 0.22M to 0.48M (about 0.8M to 1.9M raw). Plan size (bytes of `plan.md`) correlates moderately with cost (0.49 over 9 jobs) and is known when the job is created. Changed lines separate the jobs more cleanly but are known only afterwards.
- All existing jobs predate sub-project A's fix, so their output tokens are under-counted. A's lines carry `usage_v` 2, older lines do not.
- The driver is Rust and the dashboard server only reads files, so the report costs no LLM tokens.

## Decisions

- The measure is weighted tokens (input + cache creation + output + cache read / 10), the same as the budget. Raw tokens appear only in the CLI table.
- Size groups come from the plan size, because it is the only size known when the estimate is made: `small` is under 12 KB of `plan.md`, `medium` is 12 KB to 25 KB, `complex` is over 25 KB. The two thresholds are config keys `size_small_max_bytes` (default 12288) and `size_medium_max_bytes` (default 25600).
- The report also shows each job's actual changed lines and files, so the user can see where plan size misled.
- Old jobs are included and marked approximate (`~`) when any of their `usage.jsonl` lines lacks `usage_v` 2. Each group figure says how many of its jobs are approximate. When corrected jobs exist, the group shows corrected-only figures next to the combined ones.
- Only merged jobs feed the group stats. Parked and cancelled jobs appear in the per-job list with their stage. A job with no `usage.jsonl` or no `plan.md` is listed as "no data" and left out of the stats.
- Median of an even count is the mean of the two middle values. Low and high are the smallest and largest. A group with fewer than 3 jobs is marked "few jobs". A group with no jobs falls back to the overall figures over all sizes, labeled "all sizes".
- The estimate for a new job comes from the history at that moment, never including the job itself.
- No dollar amounts. No LLM calls are added.
- The dashboard stays read-only. It reads `costs.json` and the board, and never writes either.
- The dashboard panel follows the dashboard's existing look (one monospace face, cool-grey oklch tokens, hairline borders, 10px radius). No new fonts or colors.
- A job-cost record is derived on demand from the board, the job folder and git. It is not stored per job. Only the derived `costs.json` is written.

## Design

### 1. Cost model (factory-core and config)

A new `cost` module in `factory-core`, pure functions over plain data:

- `SizeGroup` (`small`, `medium`, `complex`) with `as_str`, and `group_of(plan_bytes, &Config) -> SizeGroup`.
- `JobCost`: `id`, `slug`, `stage`, `group` (optional), `plan_bytes`, `tasks`, `lines` and `files` (optional), `weighted`, `raw`, `runs`, `approx`, and `roles` (weighted tokens per role). A job with `runs` 0 is "no data".
- `GroupStats`: `n`, `n_approx`, `median`, `low`, `high`, `roles` (share of weighted tokens per role as a whole-percent map), and `corrected` (the same figures over jobs that are not approximate, or `null` when there are none).
- `Estimate`: `group` (the group name or `all`), `median`, `low`, `high`, `n`, `n_approx`.
- `stats(jobs: &[JobCost]) -> BTreeMap<String, GroupStats>` over merged jobs with data, plus an `all` entry, and `estimate_for(plan_bytes, &stats, &Config) -> Option<Estimate>` with the fallback above.

`Config` gains `size_small_max_bytes` and `size_medium_max_bytes`. A typo or an invalid value fails closed like the other config keys, and `small >= medium` is an error.

### 2. Gathering the history (factory-cli)

`gather_costs(driver) -> Vec<JobCost>` in `factory-cli` reads, per board job:

- `plan.md` in the job folder for `plan_bytes` and `tasks` (count of `### Task N:` headings),
- `usage.jsonl` for `weighted`, `raw`, `runs`, `roles`, and `approx`,
- the diff between the board's `base` and `head` for `lines` and `files`, through a new `Repo::diff_numstat(base, head)` in `factory-git`. A missing commit gives `None` and never fails the report.

### 3. Commands and the board

- `factory costs` prints a per-job table (id, stage, group, tasks, lines, weighted, raw, runs, `~` for approximate), then a table per group (jobs, median, low, high, `few jobs`, approximate count, corrected figures when they exist), then the role split. It also writes `.superpowers/factory/costs.json`.
- `costs.json` holds `generated_at`, the thresholds, `groups` (the stats, including `all`) and `roles`. It does not hold the per-job list.
- The driver rebuilds `costs.json` after a job merges, best effort: a failure never affects the merge, as with the knowledge-base refresh.
- `factory new` computes the `Estimate` for each job it creates from its plan, prints it, and stores it as the optional board field `estimate` (`group`, `median`, `low`, `high`, `n`, `n_approx`). Old boards without the field load as before.

### 4. Dashboard

- `/api/state` gains `costs` (the `groups` and `roles` of `costs.json`, or `null` when the file is missing or unreadable). It never carries the per-job list.
- A cost panel below the board: one row per group (jobs, median, range, `few jobs` or `~` where they apply), then the role split as one stacked bar with its legend as text.
- A card that is not merged and has an `estimate` shows one line, for example "Estimated 0.25M to 0.31M weighted (small, 4 jobs)". A merged card with an `estimate` shows its actual weighted cost next to the estimate range. A card without an `estimate` shows neither.
- The panel and the lines are built with the frontend-design skill in the dashboard's existing look. Bar widths go through the CSSOM because the CSP forbids inline styles.

## Testing

- `factory-core`: `group_of` at both thresholds and just below and above; median of odd and even counts; low and high; a one-job group is `few jobs`; role shares add to 100 within rounding; `corrected` is `null` without corrected jobs and set with them; the fallback to `all` when a group is empty; `estimate_for` never includes the job being estimated; config rejects `small >= medium` and unknown keys.
- `factory-cli`: `gather_costs` over a temp job folder with old and new `usage.jsonl` lines (approximate versus not), a job with no usage, a job with no plan, and a missing commit; `factory costs` output and the written `costs.json` shape; `factory new` stores `estimate` and prints it; a board without `estimate` still loads.
- `factory-engine`: the post-merge refresh writes `costs.json`, and a write failure does not fail the merge.
- `factory-server`: `costs` appears in `/api/state` when the file is valid and is `null` when it is missing or broken; a malformed `estimate` on a board job is dropped.
- Dashboard JS tests: the panel rows and the role bar render with escaped text; the estimate line shows on unmerged cards, the comparison on merged cards, nothing without `estimate`; `few jobs` and `~` markers appear.
- A manual look in a real window (light, dark and phone width) is left to the user.

## Acceptance criteria

- `factory costs` lists every job, groups merged jobs by plan size, and prints median, range, job count and approximate count per group, plus the role split, and writes `costs.json`.
- Old, under-counted jobs are marked `~` and counted as approximate, and corrected figures appear separately once corrected jobs exist.
- `factory new` prints an estimate for each new job and stores it on the board, falling back to the overall figures when the group has no jobs, and says "few jobs" when a group has fewer than 3.
- The size thresholds are config keys and invalid values fail closed.
- The dashboard shows the cost panel and the estimate and comparison lines, reads `costs.json` without writing it, and `/api/state` carries only the group and role summary.
- The post-merge refresh of `costs.json` never fails a merge.
- No dollar amounts and no LLM calls are added, and an old board and an old `costs.json` absence both load.
- `.claude/software-factory.md` documents the size groups, `factory costs`, `costs.json`, the estimate and the dashboard panel.
