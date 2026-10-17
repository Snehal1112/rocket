# Factory ticket hygiene: follow-up tickets and closing the parent

Requested after job 012 left GitHub issue #76 open although it was only partly done. The user prefers a clean backlog: a new ticket for each remaining piece and the finished ticket closed. Today the factory only writes footer text and never contacts GitHub.

## Goal

After the last plan of a series merges, the factory files one GitHub ticket per remaining piece of work, comments on the parent ticket and closes it.

## Background

Verified against `main` at `f892d8c9`:

- The driver reads `#N` tokens from the `factory new` feature text only (`factory_core::parse_issues`, `commitmsg.rs`). It does not read issue URLs, so job 012, created with a URL, got no issue footer at all.
- Issue numbers are stored as `Job.issues` and written as text footers: `Relates to: #N` on commits and syncs, `Fixes: #N` on merge commits. GitHub closes an issue by itself only when someone pushes a merge that carries a closing keyword.
- The driver never contacts GitHub. No command, skill, hook or script in the repository creates or closes issues.
- `gh` 2.101.0 is installed and logged in with `repo` scope. No rule or hook forbids it. The shell-out rules in `CLAUDE.md` and `tools/factory/CLAUDE.md` name `git` only, and the driver convention is to reach outside tools through a port in `ports.rs`.
- After a merge, `merge_stage` (`factory-engine/src/merge.rs`) already runs best-effort steps (knowledge base refresh, costs refresh) whose failure never changes the merge result.
- Specs have a `## Non-goals` section, but many non-goals are things that are never wanted, so it is not a list of remaining work.
- Ticket labels follow a convention: `dedup`, `refactor`, `tier-N`, `size-*`, and `tracking` for tracker tickets.

## Decisions

Each topic lists the question that was asked and the answer given.

- When it runs. Question: at merge time always, on demand only, or both? Answer: an on-demand command `factory tickets <id>` plus an optional merge-time hook behind the config key `tickets_on_merge`, default off. Reason: merges stay local until the user pushes, so closing at merge time can claim "done" before GitHub has the code, and the user chooses per project.
- Source of the remaining work. Question: a spec section, an AI draft from the whole spec, or the Non-goals? Answer: a new spec section `## Follow-ups`, one ticket per bullet. Deterministic, no AI call, no invented scope. A missing or empty section means nothing to file and the parent is only commented on and closed. Non-goals were rejected because many are never wanted; an AI draft was rejected for cost and variance.
- Mechanism. Question: `gh` behind a port, the GitHub REST API from the driver, or an agent running `gh`? Answer: `gh` behind a `Tracker` port. It is installed and logged in, keeps tokens out of our code, and tests use a fake. The REST API was rejected because it needs an HTTP client and token handling; an agent was rejected for cost and unpredictability.
- Purpose and who benefits. Question: why and for whom? Answer: for the user, a clean backlog without manual `gh` work after each job.
- Finding the parent ticket. Question: how does the driver know which ticket a job belongs to? Answer: from the feature text, accepting `#N` and `https://github.com/<owner>/<repo>/issues/N`. `gh` runs in the main checkout. A URL that names a repository other than `origin` makes the step skip with a ledger note.
- Labels. Question: which labels do new tickets get? Answer: all labels of the parent except `tracking`.
- Idempotency. Question: what if the step runs twice or stops halfway? Answer: a marker file `tickets.json` in the job folder records every created ticket, the comment and the closes, saved after each `gh` call. A re-run continues where it stopped and never files a duplicate.
- Failure behavior. Question: what if `gh` fails? Answer: every call is best effort. The failure is written to the ledger and printed, and the merge result is never affected. The command exits non-zero when any call failed.
- Preview. Question: can the user see what will happen first? Answer: `factory tickets <id> --dry-run` prints each action and does nothing.
- Parent comment. Question: what does the comment say? Answer: a fixed template with the job id, the merge sha, the words "not yet pushed" when the sha is not on `origin/main`, and links to the new tickets.
- Merge footer. Question: should a partly done ticket still get `Fixes`? Answer: no. When the job's `spec.md` has follow-ups, the merge message uses `Relates to: #N`; otherwise it keeps `Fixes: #N`.
- Security and compatibility. Question: what must not weaken? Answer: titles and bodies are passed as separate arguments, never through a shell; a spec or board without `## Follow-ups` behaves exactly as today; no board field is added; existing footers, board JSON and `/api/state` are unchanged.
- Success checks and how each is measured. Question: how do we know it works? Answer: the acceptance criteria below, each pinned by a named test using a fake `Tracker`.
- Edge cases. Question: which cases matter? Answer: a feature with several issues, an issue given only as a URL, a URL for another repository, a job that is not the last plan, a job that is not merged, a missing `spec.md` in the job folder, a bullet spanning several lines, an empty `## Follow-ups` section, and a re-run after a partial failure.
- Scope against related tickets. Question: which tickets or branches overlap? Answer: none open. #76 was handled by hand (follow-ups #106, #107, #108). The tier trackers (#89 to #91) are not updated by this feature.
- Non-goals and rejected alternatives. See the Non-goals section.

## Non-goals

- Pull requests, branches or pushing code.
- Label management beyond copying the parent's labels, and setting sizes or priorities.
- Updating the tier tracker tickets.
- Any AI drafting of ticket text.
- More than one repository per job.
- Closing a ticket for a job that has not merged, or before the last plan of a series.
- Changing how `Relates to` footers on commits and syncs are written.

## Behavior that must not change

- A spec without `## Follow-ups` gives the same merge message as today (`Fixes: #N` when the feature text has issues).
- `parse_issues` still returns a sorted, de-duplicated list of numbers, and the board, ledger and `/api/state` formats do not change.
- With `tickets_on_merge` off (the default) no merge calls `gh`.
- A `gh` failure never turns a merged job into any other stage.

## Design

### 1. Parsing (factory-core and factory-engine)

- `parse_issue_refs(text)` returns each reference as a number plus an optional `owner/repo`, reading `#N` and `https://github.com/<owner>/<repo>/issues/N`. `parse_issues` calls it and keeps its current return type and behavior for `#N` text.
- `follow_ups(spec)` returns the bullets under `## Follow-ups`, using the existing `section_body` and `is_list_item` helpers (made crate-visible). The title is the first sentence of a bullet, cut at 100 characters; the body is the whole bullet with continuation lines joined. A missing or empty section gives an empty list.
- The merge footer key is chosen from `follow_ups(job spec)`: non-empty gives `Relates to`, empty or no spec gives `Fixes`.

### 2. The tickets step

- A `Tracker` trait in `ports.rs` with `labels_of`, `create`, `comment`, `close` and `is_closed`. `GhTracker` runs `gh` with an argument list in the main checkout and a timeout per call. Tests use `FakeTracker`.
- `tickets_step(driver, id, dry_run)` requires a merged job that has no series or is the last plan. It reads the refs from `job.feature` and skips with a message when there are none or a URL names another repository. Then it creates a ticket per follow-up that is not in `tickets.json` (labels copied, body linking to the parent tickets), comments once on each parent, and closes each parent that is still open. `tickets.json` is saved after every call.
- `factory tickets <id> [--dry-run]` in factory-cli prints each action and exits non-zero on any failure.
- `tickets_on_merge` in `config.toml`, default false. When on, `merge_stage` runs the step after the knowledge base refresh with the existing best-effort pattern.

### 3. Docs and the `/factory` command

`.claude/software-factory.md` documents the section, the command, the key and the footer rule. `.claude/commands/factory.md` adds `## Follow-ups` to the spec rules (list the remaining pieces of the ticket, or leave it out when the ticket is fully done) and asks that the issue be given as `#N` in the feature text.

## Tests and measurement

- factory-core: `parse_issue_refs` for `#N`, a URL, both together, a URL with a repository, duplicates, and text without issues; `parse_issues` unchanged.
- factory-engine: `follow_ups` for one bullet, several bullets, a multi-line bullet, no section and an empty section; the merge footer is `Relates to` with follow-ups and `Fixes` without.
- factory-engine with `FakeTracker`: creates the right tickets with copied labels minus `tracking`; comments once; closes only open parents; skips on a foreign repository, on no refs, on a job that is not merged and on a job that is not the last plan; a re-run after a failure at each of the three stages creates no duplicate; `--dry-run` calls only read methods; a failing tracker leaves a merged job merged; the hook runs only when `tickets_on_merge` is on.
- factory-cli: `factory tickets` output and exit code for success, dry run and failure.
- Manual check by the user: run `factory tickets <id> --dry-run` on a merged job with a `## Follow-ups` section.

## Follow-ups

- Update the tier tracker tickets (#89 to #91) when a ticket they list is closed by the factory.

## Acceptance criteria

- `parse_issue_refs` reads `#N` and GitHub issue URLs, and `parse_issues` behaves exactly as before for existing input.
- `follow_ups` returns one item per bullet of `## Follow-ups`, and nothing for a missing or empty section.
- The merge message uses `Relates to: #N` when the spec has follow-ups and `Fixes: #N` otherwise.
- `factory tickets <id>` on a merged last-plan job files one ticket per follow-up with the parent's labels except `tracking`, comments once on each parent and closes each open parent.
- Running it again, including after a failure at any step, creates no duplicate ticket, comment or close.
- `--dry-run` performs no create, comment or close call and prints every action it would take.
- The step skips with a clear message for a job with no issue refs, a URL for another repository, a job that is not merged and a job that is not the last plan.
- With `tickets_on_merge` off, no merge calls `gh`. With it on, a `gh` failure is written to the ledger and never changes the merge result.
- `GhTracker` passes titles and bodies as separate arguments and never runs a shell.
- `.claude/software-factory.md` and `.claude/commands/factory.md` describe the section, the command, the key and the footer rule.
