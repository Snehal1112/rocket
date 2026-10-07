# rok API parity, sub-project A: plan index

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-a-sync-design.md`
**Series notes:** `.claude/rok-api-parity-notes.md`
**Ledger:** `.superpowers/sdd/rok-parity-a/progress.md` (create on first task; log `Ruling:` lines there)

Run the plans in order. Each plan has at most 3 tasks and produces working, tested software on its own. Each plan file ends with a "Next plan to execute" section: the executing Claude moves on to the next plan on its own when the current one is complete (no consent needed between plans). After plan 02 the next work is sub-project B, which still needs a plan written from its committed spec.

| Plan | File | Tasks |
|---|---|---|
| 01 | `01-variable-reads-and-deletes.md` | read ops, delete ops with apply side, same-script read overlay |
| 02 | `02-context-and-response.md` | context fields (collection name, safe mode, cwd, `__dirname`), test and assertion results, `res` and runner extras |

## Rulings and corrections to the spec (read before starting)

These came from reading the code while planning. The spec is updated to match in the commit that adds this plan.

1. **Persistence was misdescribed in the spec.** Today every script write except `rok.setVar` already persists: `apply_script_side_effects` calls `apply_env_writes(..., force_persist = true)` for env and global writes, and collection writes are always saved. `EnvVarWrite.persist` is inert. The ruling "keep Rocket semantics" therefore means: new set and delete APIs persist like the existing ones, and `{ persist: true }` stays accepted and inert. The user was told the opposite when they chose; surface this again if they ask for in-memory behaviour, which would be a separate change.
2. **`getProcessEnv` exposes the whole host environment.** `build_variable_context_with_process_env` fills `process_env` from `std::env::vars()`. In Safe mode this lets a script from an untrusted collection read host variables. Implemented as specified, flagged in the security spec follow-up. Gating it to Developer mode is a one-line change in `bootstrap.js` if the user asks.
3. **Deferred to a follow-up: `getOauth2CredentialVar` and `resetOauth2Credential`.** OAuth2 tokens live in `OAuth2Service` and the frontend token flow, not in anything `ScriptContext` can reach. Needs its own design.
4. **`res.url` and `res.getUrl()` return the request URL.** `HttpResponse` has no final-URL field, so redirects are not reflected.
5. **`rok.runner.stopExecution()` reuses the runner's stop path.** It records `NextRequest::Stop` (and `skip_request` in the before-request phase). No runner change.
6. **`getTestResults()` returns the tests recorded so far by the current script.** Not a pre-run snapshot.
7. **`__dirname` is the collection root, `__filename` is `undefined`,** in Developer mode. The executing script's own path is not in `ScriptContext`. Per-folder `__dirname` is a follow-up. Local modules loaded with `require` already get their own `__dirname` and `__filename`.
8. **`getFolderVar` is not in this plan.** The folder-settings plan 07 owns it.
9. **Additions found while planning:** a same-script read overlay (reads see the script's own earlier writes, which they do not today), and non-string runtime variables are now kept as JSON text instead of silently dropped when merged between phases.

## Global constraints (apply to every task)

- Hook rules (`.claude/rules/harness.md`): `cargo test|check|clippy|build` must pass `-j4`. No `cargo test --workspace`. No `git add -A`, `--all` or `.`.
- Commits: use the `dev-workflow-skills:1-git-commit` skill with explicit paths and a pathspec commit (`git commit --only ... -- <paths>`). Conventional commit prefixes.
- Before a commit that stages `.rs`, `cargo check -j4` must pass. For `.ts` or `.tsx`, `yarn tsc --noEmit` and `yarn check`.
- Rust: no panicking calls in production paths (use `DomainResult` and `ScriptOpError`). No `rename_all = "camelCase"` on persistence structs.
- Keep `crates/rocket-infra/src/scripting/bootstrap.js` and `src/components/editor/rok-types.ts` in sync. Task 1 of plan 01 adds a test that enforces it.
- Missing-key convention: existing getters return `""` for a missing key. New getters follow it, except `getProcessEnv`, which returns `undefined` as in Bruno.
- Code comments: short full sentences ending in a period.
- 📖 The spec lists `opencollection-spec-reference.md` as required reading because collection variable persistence is touched. Read `docs/superpowers/specs/opencollection-spec-reference.md` before plan 01 task 2.

## Verification after each plan

```bash
cargo check -j4
cargo test -j4 -p rocket-scripting
cargo test -j4 -p rocket-infra scripting
cargo test -j4 -p rocket-app execution_service
yarn tsc --noEmit
yarn check
yarn test rok-types
```
