# rok.grpc Implementation Plans (index)

Spec: `docs/superpowers/specs/2026-11-12-rok-grpc-design.md` (approved). Read it first.

| Plan | Scope | Tasks |
|---|---|---|
| [01-core-exchange.md](01-core-exchange.md) | Host types, limits, confined proto path, `GrpcService::exchange` for all four method types. No script surface. | 3 |
| [02-script-surface.md](02-script-surface.md) | `HostCall::Grpc`, op, `rok.grpc.run` / `rok.grpc.call`, `ExecutionScriptHost::grpc`, wiring, end-to-end tests. | 3 |
| [03-typings-and-docs.md](03-typings-and-docs.md) | Typings, feature doc, manual test, crate guides, parity notes, roadmap. | 2 |

Run the plans in order. Each ends with working, tested code.

## Rulings recorded while planning

- **Ruling 1 (Developer mode check lives in the op).** `rok.isSafeMode()` already reads `OpState` `sandbox_mode` in `rocket-infra`. The op refuses in Safe mode before any host call, so `ScriptHost` and `ExecutionScriptHost` never see the mode. Cost if wrong: none, the host can add a second check later.
- **Ruling 2 (confined proto path is a new mode, the pane keeps its rules).** The gRPC pane accepts absolute and `~/` proto paths (`GrpcService::resolve_proto_path`). Script calls use a stricter mode: a relative path inside the collection, symlinks resolved through a new `ProtoLoader::resolve_within`. The pane is unchanged. Cost if wrong: a script cannot use a proto outside the collection, which is the intent.
- **Ruling 3 (variables).** `rok.sendRequest` resolves no variables. `rok.grpc.*` fills `{{var}}` in the saved request or ad hoc fields from `send_variable_map` plus the calling script's `runtime_vars`, as the pane does. Cost if wrong: a missing variable becomes an error naming the variable, not its value.
- **Ruling 4 (nested budget).** A gRPC call takes one `ActiveRun` slot while it runs, so it shares the `MAX_ACTIVE_NESTED_RUNS` limit with `rok.runRequest`.
- **Ruling 5 (commits).** Implementers stage explicit paths and never commit. The controller commits through the commit skill: new commit, no amend, no split.
- **Ruling 6 (literal text).** A repo hook blocks any write that contains the literal text for an unwrap call. Use `expect("reason")` in tests and error returns in production.

## Standing constraints for every task

- `cargo` commands take `-j4`, never `--workspace` or `--all`. Use this worktree's own target dir.
- Frontend: Bun only (`bun run tsc --noEmit`, `bun run check`, `bun run test <pattern>`).
- Tasks that touch gRPC requests, collections or the script host start with: **Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**
- DDD: domain crates hold logic and traits only. `rocket-infra` implements ports and defines no trait. `rocket-app` holds `Box<dyn Trait>` or `Arc<dyn Trait>` and reads no files. `src-tauri` stays thin.
- Comments are short full sentences ending with a period. No emojis.
- Before merge: code builds and no linter errors.
