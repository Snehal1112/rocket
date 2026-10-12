# Flow Tier 2 plan index: silent failures and detection gaps

Source: `.claude/flow-roadmap.md` Tier 2. F-20 and F-23 are already done. These five plans cover the nine open items F-21, F-22, F-24, F-25, F-26, F-27, F-28, F-29 and F-30. Each plan has 3 tasks, tests first, real code, and one commit per task through the `dev-workflow-skills:1-git-commit` skill.

Written 2026-10-11 against `main` at `c188b536` (Tier 1 merged). None of the code in these plans has been compiled or run. Each plan starts its tasks by re-reading the files as the earlier plan left them, because plans were written in parallel.

## Plans and order

| Plan | File | Items | Depends on |
|---|---|---|---|
| 1 | `01-lint-context-and-rules.md` | F-21 dangling saved request, F-22 unknown variable, the app-layer `CollectionLintContext` | none |
| 2 | `02-lint-rules-auth-callback.md` | F-24 Auth lints, F-25 callback ordering for saved requests | Plan 1 |
| 3 | `03-hard-errors.md` | F-26 Request wire field whitelist, F-27 duplicate edge ids and Output wiring | none |
| 4 | `04-all-validation-errors.md` | F-28 `validate_all`, clearer messages, multi-error display | Plan 3 |
| 5 | `05-corrupt-flows-and-step-guard.md` | F-29 corrupt flows in the list, F-30 `MAX_RUN_STEPS` | none |

Recommended execution order: 1, 2, 3, 4, 5. Plans 1+2, 3+4 and 5 are independent chains, but only one implementer runs in a worktree at a time. Plans 3 and 4 both edit `validate.rs` and `graph.rs`, so keep them in order. Use a new worktree (for example `flow-tier2`) and give it its own cargo target dir.

Next plan to execute: `01-lint-context-and-rules.md`.

## Rulings made while reconciling the plans

- Callback ordering (F-25) follows the existing V15 rule: the request that registers `{{callback.x}}` must be upstream of the Wait x. The first brief said the opposite, and the Plan 2 writer corrected it.
- Plan 2's tests use Plan 1's test helpers (`sources`, `flow_of`, `inline_node`, `ENV`) and Plan 1's constructor `CollectionLintContext::new(&LintSources, collection, LintEnvironment, &Flow)`. I fixed the two spots in Plan 2 that guessed a different constructor.
- Plan 2's repeated reads of a saved request (decision 7) are already covered: Plan 1's context caches the existence answer per path. Plan 2 only adds reads for auth and callbacks.
- Plan 4 keeps the string error format and does NOT do F-50 (structured IPC errors, Tier 4).

## Risks to know before executing

1. Plan 3 turns four things into hard errors that block save and run: an unknown Request wire field, a repeated edge id, an Output with zero wires, an Output with more than one wire. Flows already on disk that break these rules will no longer save or run. There is no migration. The plan pins three existing test fixtures that break and tells the implementer to fix the others without loosening a rule.
2. Plan 1's `dangling_saved_request` is an Error-severity lint, not a hard error. It never blocks save or run.
3. Plan 1 makes the `lint_flow` command `async` and adds optional environment arguments. Three existing frontend assertions change shape.
4. Plan 4's test pins an exact error list in two tests. If Plan 3's new rules also fire on those fixtures, adjust the tests.
5. The F-22 rule stays silent when the flow has a Saved Request node, no environment is selected, or the environment extends another. It reports only when it is certain, so it will miss some real mistakes.
6. `.claude/flow-roadmap.md` is untracked on `main`. The plans say to mark items done but to leave that file out of commit pathspecs.

## Open decisions (recommended default first)

Plan 1
1. Dangling saved request severity: Error. 2. No active environment: skip names it cannot find. 3. Any Saved Request node silences the unknown-variable rule for the whole flow. 4. Linting the fields of saved requests: not in this plan. 5. Re-lint on open, edit, environment change and save only. 6. Global environment name read at render time. 7. `lint_flow` becomes async. 8. Variable-name filter: letters, digits, `_`, `-`, `.`, at most 64 characters.

Plan 2
1. Follow V15 for callback direction. 2. Run-start engine check: out of scope. 3. Scripts, auth fields and folder or collection fields are not scanned. 4. The auth-wire lint is a wire-level warning. 5. An unreadable saved request is silent. 6. A request with `none` or `inherit` auth counts as taking the inherited credential. 7. Repeated reads: accept. 8. The callback rule does one forward walk per request that uses a callback, so it is not strictly linear: accept.

Plan 3
1. Zero Output value wires is an error (alternative: warning, with only "more than one" an error). 2. New `DuplicateEdge` variant. 3. The saved-request body-type check is deferred to a `LintContext` warning (rocket-flow cannot read saved requests, and inline bodies are always JSON). 4. Output field names are not whitelisted. 5. No migration or file scan.

Plan 4
1. Errors grouped in rule-table order. 2. Cap at 50 with the cut count in the header. 3. Keep the tagged string until F-50. 4. Server-side masking of match values and exit names stays out of scope. 5. The V15 reason still names the Request by id.

Plan 5
1. Rename of a corrupt flow is disabled (it rewrites the name inside the unparseable file). 2. A corrupt file with a non-slug stem stays hidden. 3. One optional `error` field is the corrupt signal. 4. Saving over a corrupt file is left alone and guarded in the picker. 5. `MAX_RUN_STEPS` is 10,000, checked against the node count at run start. 6. No per-step counter inside the run loop until a loop-forming node exists.

## Not verified yet

- serde_yaml 0.9 error wording for a wrongly typed `nodes` field (Plan 5), the exact accessible names of the issues button (Plan 1), `FormDataType::Text` (Plan 2), and the `if let ([only], 0) = (...)` pattern in Plan 4 (it has a fallback).
- Whether the plans, written in parallel, overlap in `lint.rs`, `flow_service.rs` or `flow-issues.ts`. The first implementer of each later plan should check with `git diff` before editing.

## After the plans

Per plan: implementer, then an Opus read-only review of the diff, then a fix pass. After Plan 5: whole-branch review, full gates, roadmap update, ask before merging to `main`. Follow-ups already logged: F-123 to F-132 in the roadmap.
