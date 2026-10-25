# rok API parity B review fixes: plan index

**Source:** the Opus review of Part B (R-02), range `abbe0a17..f5c32a79`, 2026-10-25. Part B spec: `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`. Part B plans: `docs/superpowers/plans/rok-parity-b/` (its index holds the rulings these plans build on).
**Ledger:** `.superpowers/sdd/rok-parity-b-fixes-<plan file name without .md>/progress.md` (create it on the plan's first task; log `Ruling:` lines there).

Run the plans in order. Each plan has at most 3 tasks and produces working, tested software on its own. When a plan is complete, move on to the next one without asking.

| Plan | File | Tasks |
|---|---|---|
| 01 | `01-guard-and-vault.md` | `rok.sendRequest` obeys the workspace internal-host guard; nested `rok.runRequest` runs fail closed on an unresolved RocketVault alias |
| 02 | `02-limits-and-docs.md` | cap on concurrent host calls per script and on active nested runs per send; roadmap and series-notes update |

## Rulings (read before starting)

1. **Review issue 1, scope.** Only `rok.sendRequest` gets the guard. A `rok.runRequest` target is a request the user saved, so its URL is the user's own and the guard never inspects typed URLs (see the doc comment on `check_request_guard`). The nested run already receives `request_guard_policy` through `build_step_input`, so a `req.setUrl` inside it is guarded as in any send. Plan 01 Task 1 pins that with a test instead of adding a second check.
2. **Guard exemption.** `check_request_guard` lets a script reach the host the calling request already uses. `rok.sendRequest` keeps that rule, with the calling request's URL as the original. A calling URL that is a `{{template}}` does not parse, so it gives no exemption.
3. **Vault failures travel with the run.** `execute_unpinned` is the only caller of `resolve_external_secrets_partial` that tolerates failures (the Flow and runner paths use the strict resolver). It hands the unresolved aliases down to the script host, so a nested run can refuse a send that refers to one, as `execute` does. No second vault fetch per `runRequest` call.
4. **Concurrency limits.** Two layers. Per script: at most `ScriptLimits.max_host_calls` (default 8) host calls are served at once; the rest wait in the call channel. Per top-level send: at most 16 nested runs active at once, counted in an `Arc<AtomicUsize>` shared down the chain. Over the limit, `rok.runRequest` rejects at once. It never blocks, because a parent holding a permit while waiting for its children could deadlock.

## Next plan to execute

Plan 01 first. After plan 02 the next work is sub-project C (cookies): spec `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`, no plan yet. Invoke `superpowers:writing-plans` and write it under `docs/superpowers/plans/rok-parity-c/`.
