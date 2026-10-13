# Software Factory for Rocket

Status: built (plans 01 to 03 in `docs/superpowers/plans/2026-10-13-software-factory/`). Run the dashboard with `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$(python3 .claude/factory/factory-ctl root)"` and use `/factory <feature>` in an interactive session. Pipeline from the user's chart (2026-10-12), fitted to Rocket's rules on 2026-10-13.

## Pipeline

```
/factory <feature> -> spec-writer (opus) -> builder (opus, own worktree) -> 4 reviewers in parallel -> approver (opus)
                                                  ^                              |                       |-- APPROVE  -> merge to local main
                                                  |   any CHANGES (max 2)        |                       |-- ESCALATE -> needs-human
                                                  +------ orchestrator resumes ---+                           (/factory approve | /factory rework)
```

Reviewers: security (opus), ux (sonnet), ui design (sonnet), code (sonnet). The orchestrator is the interactive session. It dispatches agents, owns the round counter, writes the ledger and the board, and does the merge. It never does builder or reviewer work.

## Job model

- One job is one plan of at most 3 tasks. A feature that needs more is split into several plans, run one after another, never in parallel.
- Job id `NNN`, branch `factory/NNN-<slug>`, worktree `.claude/worktrees/factory-NNN-<slug>` (already gitignored, `settings.json` has `worktree.baseRef: head`).
- The plan folder name starts with the creation date, `YYYY-MM-DD-NNN-<slug>`, as older SDD folders already do. The orchestrator sets it when the job is created and never renames it. The board card shows the date too.
- Files per job, all under gitignored `.superpowers/`:
  - `.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/progress.md`: the normal SDD ledger. Lines use the strict form `Task N: dispatched | implementer done | complete`, plus `Ruling:` lines, so `resume-plan` keeps working after `/clear`.
  - `.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/spec.md`, `plan.md`, `task-N-brief.md`, `review-<base>..<head>.diff`, `review-<role>.md`, `approval.md`.
  - `plan.md` lists every task as a checkbox line, `- [ ] Task N: <title>`. The spec writer creates them unchecked.
  - `.superpowers/factory/board.json`: stage per job, written only by the orchestrator, read by the dashboard server.

## Agents (`.claude/agents/factory-*.md`)

| Agent | Model | Tools | Rocket duties |
|---|---|---|---|
| `factory-spec-writer` | opus | read-only plus write to the job dir | Writes spec and plan files of at most 3 tasks. Prepends the OpenCollection read line to every task touching yml, collection, environment, auth, variable resolution or the Rust crates named in `CLAUDE.md`. |
| `factory-builder` | opus | full, `isolation: worktree`, includes `Skill` | One implementer per worktree. Follows the matching rule file (`rust-ddd-boundaries`, `tauri-ipc-boundaries`, `frontend-component-guardrails`). Runs its own checks (see below). Commits with `dev-workflow-skills:1-git-commit` and a pathspec. |
| `factory-security-reviewer` | opus | read-only | Reads the diff file. Checks the collection trust gate, `{{process.env}}`, secrets and masking, auth, no git CLI, no unwrap calls in production paths, CSP, IPC surface. |
| `factory-ux-reviewer` | sonnet | read-only | Runs only when the diff touches `src/`. Flows, empty and error states, keyboard use. |
| `factory-ui-reviewer` | sonnet | read-only | Runs only when the diff touches `src/`. shadcn primitives only (no raw `button`, `input`, `dialog`, `select`, `form`), lucide icons only, `SingleLineEditor` vs Monaco, narrow Zustand selectors. |
| `factory-code-reviewer` | sonnet | read-only | DDD placement, serde rules (camelCase on IPC DTOs only), error handling, persistence backward compatibility, tests present. |
| `factory-approver` | opus | read-only | Reads all verdict files and the diff, applies the risk rules, writes `approval.md`. |

Models are fixed in agent frontmatter, so no dispatch can inherit the expensive session model by accident. The user's usual default for builders is Sonnet, but they decided on 2026-10-13 that the builder uses Opus, as in the chart.

There is no separate verifier agent, matching the chart. The builder runs the checks before it reports.

## Compatibility with Rocket's harness

| Rocket rule | How the factory complies |
|---|---|
| `cargo` must pass `-j4`, no `--workspace` tests | Builder uses `cargo check -j4` and `cargo test -j4 -p <crate> <test>`. It must not use `/verify-rocket` as written, because that command runs `cargo check` without `-j4` and `cargo test --workspace --no-run`, and `bash-guard` would deny both. Fixing that command is a separate small task. |
| Commit gate runs `yarn tsc --noEmit`, `yarn check`, `cargo check -j4` | Builder commits normally. `HARNESS_GATE=0` is never set by an agent. |
| Commit gate checks only the root workspace | The factory server lives outside it, so tasks touching `tools/factory-server/` also run `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`. |
| No `git add -A`, `--all` or `.`, no bare `git stash` | Pathspec commits only: `git add <paths> && git commit --only -m ... -- <paths>`. |
| Commits go through `dev-workflow-skills:1-git-commit`, conventional format | Written into the builder prompt, and `Skill` is in its tool list. |
| One implementer per worktree, reviewers read a diff file | The orchestrator writes `review-<base>..<head>.diff` and passes only that path to the reviewers. |
| Re-reviews are scoped to fixes since the last review | After a CHANGES round, reviewers get a new diff of only the fix commits. |
| A worktree gets its own cargo target dir | Never set `CARGO_TARGET_DIR` to the main `target/`. First build in a worktree is slow (V8 link, lld). |
| Machine is easily overloaded | Only one builder runs at a time. Reviewers are read-only and never run cargo or yarn. |
| `rust-safety-pretool` blocks the banned unwrap pattern in any Rust write | The server crate is written without it too. |
| Peer sessions share the main checkout's git state | The merge step (below) refuses to run on a dirty or moved main. |
| Model must be stated at dispatch | Fixed in each agent file. |
| Hard rules: shadcn, lucide, Monaco, Zustand, no git CLI, serde | Encoded as reviewer checklists, and as builder instructions. |

## Merge step

Done by the orchestrator, only after the approver says `APPROVE`:
0. If the branch tip is already an ancestor of `main` (`git merge-base --is-ancestor factory/NNN-<slug> main`), the human merged it by hand. Skip the merge and go straight to step 3, so `/factory approve` can be run again after a manual merge. The orchestrator uses the head of `main` at that moment as the merge reference. `factory-ctl is-merged` checks the recorded job head commit, not the branch name, so it still works if the branch was deleted after a manual merge. A job with no commits is never reported as merged.
1. Main checkout must be on `main`, clean (`git status` empty for tracked files), and `HEAD` must still be the job's base or an ancestor of the branch tip. Otherwise the job goes to `needs-human` with the reason "main busy". The human can then merge by hand and run `/factory approve` again.
2. Merge with `git merge --no-ff factory/NNN-<slug>`. Never push, never rebase or rewrite history that is not the job's own.
3. Record `Task N: complete (commits <range>, review clean)` in the ledger, set the board stage to Merged. The worktree is kept until the user removes it.

## Approver risk rules

Escalate to a human when the diff touches any of:
- the collection trust gate, auth, secrets, or `{{process.env}}` handling
- persistence formats (`.yml` schemas, serde persistence structs)
- the Tauri IPC surface or `DomainEvent` variants
- `Cargo.toml`, `package.json`, `tauri.conf.json`, CSP, or the harness hooks
- any reviewer `CHANGES` that was overridden instead of fixed

It also escalates when round 2 still has `CHANGES`, and when the main checkout was busy at merge time.

## Verdict contract

Every reviewer is read-only and returns its report as its reply. The report is numbered findings with `file:line`, and its last line is exactly `VERDICT: APPROVE` or `VERDICT: CHANGES`. The orchestrator saves the reply as `review-<role>-r<round>.md` and parses only that line. A missing or malformed verdict counts as CHANGES. The approver ends with `DECISION: APPROVE` or `DECISION: ESCALATE`, and anything else counts as ESCALATE. Reviewers skipped because the diff has no `src/` changes are recorded as `SKIPPED`, which counts as pass.

## Commands (`.claude/commands/factory.md`)

- `/factory <feature>`: create a job, run the spec writer.
- `/factory next`: advance the oldest job one stage.
- `/factory approve <job-id>`: release a `needs-human` job to the merge step. When the merge succeeds, the orchestrator changes every `- [ ] Task N` line in that job's `plan.md` to `- [x] Task N`, and writes the matching `Task N: complete` ledger lines. If the merge is refused ("main busy"), nothing is checked and the job stays in `needs-human`. The same ticking happens when the approver auto-approves a low-risk job.
- `/factory rework <job-id> <note>`: send a `needs-human` job back to the builder with the note. Does not reset the round counter.
- `/factory status`: print the board in the terminal (same data as the dashboard).

## Live tracking dashboard

Yes, it tracks every job stage by stage. It lives outside the Rocket app so it is never shipped.

```
Claude Code hooks (async) -> .superpowers/factory/events.jsonl -> factory-server (Rust, localhost) -> dashboard.html
                              ^ also watched: .superpowers/factory/board.json
```

Hooks append to a local JSONL file instead of POSTing, so a stopped server can never affect an agent. Plans: `docs/superpowers/plans/2026-10-13-software-factory/`.

- Columns: `Spec`, `Build`, `Review` (sec, ux, ui, code), `Approve`, `Merged`, `Needs human`.
- Card: job id and slug, feature text, branch, builder round 0 to 2, four review chips (pending, running, pass, changes, skipped), time in stage.
- Live detail on a card: current tool and tool count, last message. Hook events are matched to a job by the `cwd` of the worktree.
- Stage comes from `board.json`. Live detail comes from `SubagentStart`, `SubagentStop`, `PostToolUse`. No cost tracking in v1.
- Location `tools/factory-server/`: own Cargo workspace, not in the root `members`, own target dir. Rust only, no banned unwrap pattern.
- The log is never rotated. To rotate, move or truncate `.superpowers/factory/events.jsonl` (the server notices a replaced file).
- Hooks are async and the server may be down. An agent must never be blocked by the dashboard.

## Open items

- Settled: `PostToolUse` inside a subagent carries `agent_id` and `agent_type` (probe done 2026-10-13).
- Settled: no http hook is used, so there is nothing to check.
- `Skill` inside a subagent is only reported by the subagent itself. The builder calls the git-commit skill; if it cannot, the orchestrator commits.
- Confirm in the first real run that the Agent tool id matches the hook `agent_id`.
- The dashboard look has not been checked in a browser.
- Older ledgers are inconsistent. The server parses only the strict `Task N:` lines and ignores the rest.
- `/verify-rocket` conflicts with the harness guard. Fix separately.

## claude headless probe (2026-10-13)

Version: `2.1.296 (Claude Code)`. Probed by `tools/factory/crates/factory-agent/examples/probe.rs`: Haiku, `--max-budget-usd 0.05`, temp directory, nine calls, total cost about 0.015 USD.

Raw findings (shortened):

- `--help` lists every flag except `--max-turns`.
- stream-json without `--verbose`: exit 1, `When using --print, --output-format=stream-json requires --verbose`.
- stream-json with `--verbose`, prompt on stdin: exit 0, line types `system`, `assistant`, `result`. One session id. The `result` line carries `usage`, `session_id`, `total_cost_usd`, `stop_reason`.
- `--max-turns 1`: exit 0, so the flag is accepted although it is hidden from `--help`. Enforcement was not tested, because the reply needed one turn. The driver counts turns itself either way.
- `--append-system-prompt`: exit 0. That run had 2 assistant lines with 1 unique message id, so the same message id does repeat. Dedup by id is required.
- `--permission-mode acceptEdits` with `--allowedTools Read,Bash`: exit 0, a tool turn ran (`user` lines appeared, 2 unique message ids).
- `--agent probe` with `.claude/agents/probe.md`: exit 0, no error. The reply text was not inspected, so "loads the agent file" rests on the flag being accepted and the larger prompt.
- `--resume <id>`: exit 0, and the session id in the stream stayed the same as the original.
- Stderr carries a warning about `ANTHROPIC_API_KEY` taking precedence over the claude.ai login. It is harmless noise.
- Not inspected by the probe: whether each assistant line carries `message.usage`. The parser falls back to the `result` usage when none is seen.

| Question | Answer |
|---|---|
| `-p` works with the prompt on stdin | yes |
| `--output-format stream-json` needs `--verbose` | yes |
| `--agent <name>` loads `.claude/agents/<name>.md` | yes (accepted, reply text not checked) |
| `--resume <id>` continues, and the session id stays the same | yes |
| `--permission-mode acceptEdits` and `--allowedTools` accepted | yes |
| `--model` accepted | yes |
| `--max-turns` accepted | yes (hidden from `--help`, enforcement not tested) |
| `--append-system-prompt` accepted | yes |
| `system/init` line carries `session_id` | yes |
| Final line has `type:"result"` with `result`, `is_error`, `subtype`, `session_id`, `usage` | yes |
| Assistant lines carry `message.id` and `message.usage` | `message.id` yes, `message.usage` unknown |
| One message id appears on several assistant lines | yes |

Fallback rulings, and which apply:

- `--agent` missing: does not apply. `Caps.agent = true`. If it ever disappears, `AgentSpec.agent_prompt` goes through `--append-system-prompt`, then into the stdin prompt.
- `--max-turns` missing from help: applies only to the docs. It is accepted, so `Caps.max_turns = true`. The driver still counts unique assistant message ids and kills past the limit (`StopReason::TurnLimit`).
- `--verbose` is required for stream-json: always passed.
- `--resume` kept the session id here. The driver still takes the latest `session_id` seen, so the engine stores it after every run.
- `--permission-mode` exists: headless builds are allowed.

`Caps::default()` is `agent: true, max_turns: true, append_system_prompt: true`.
