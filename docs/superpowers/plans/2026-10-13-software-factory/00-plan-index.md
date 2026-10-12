# Software Factory Plan Index

Created 2026-10-13. Spec: [.claude/software-factory.md](../../../../.claude/software-factory.md).

Run the plans in order, one at a time. Each plan has at most 3 tasks.

| Plan | File | Delivers | Status |
|---|---|---|---|
| 01 | [2026-10-13-software-factory-plan-01-agents-hooks-command.md](2026-10-13-software-factory-plan-01-agents-hooks-command.md) | Event logger hook, 7 factory agents, `factory-ctl` helper, `/factory` command | done |
| 02 | [2026-10-13-software-factory-plan-02-rust-server.md](2026-10-13-software-factory-plan-02-rust-server.md) | `tools/factory-server/` (Rust): tails `events.jsonl`, reads `board.json`, serves `/api/state` | done |
| 03 | [2026-10-13-software-factory-plan-03-dashboard.md](2026-10-13-software-factory-plan-03-dashboard.md) | Dashboard page (stage board, review chips, live agent detail, live feed), smoke test, docs | done |

Plan 02 starts with a gate: it reads the probe results from plan 01 task 1. If tool events carry no `agent_id`, the server still works but the cards show agent start and stop without the live tool line.

## Rulings

- Ruling: hooks append trimmed events to `.superpowers/factory/events.jsonl` instead of POSTing to the server — cost if wrong: the server must tail a file, which is slightly less instant than a push.
- Ruling: the board and job helper (`factory-ctl`) is a small Python script, like the existing harness hooks, so it can be tested with plain bash — cost if wrong: one more Python file in a Rust and TypeScript repo.
- Ruling: the builder uses Opus (user decision 2026-10-13).
- Ruling: plan folders carry the creation date; plan tasks are checkboxes that `/factory approve` ticks after the merge.
- Ruling: the server polls files (500 ms) and the page polls the API (1 s) instead of using a file-watcher crate, SSE or WebSockets — cost if wrong: up to a second of delay, and a few more requests on localhost.
- Ruling: the dashboard is plain HTML, CSS and vanilla JavaScript embedded in the Rust binary, not React and TypeScript — cost if wrong: it does not follow the usual frontend stack, but it needs no build step and is not part of the Rocket app.
- Ruling: agents are attached to jobs by builder id, then worktree path, then stage and start time, because read-only agents run in the main checkout — cost if wrong: if two jobs sit in the same stage, an agent can attach to the older one.

## Probe results

Probed 2026-10-13 with a headless `claude -p --model haiku` run started inside the worktree, because hooks added to `.claude/settings.json` on a branch are not loaded by an already-running session.

- `PostToolUse` inside a subagent carried `agent_id` and `agent_type` (`general-purpose`). The orchestrator's own `PostToolUse` carried neither. `SubagentStart` and `SubagentStop` carried both. Plan 02 can rely on `agent_id` for live tool detail.
- A haiku subagent reported that a `Skill` tool is in its tool list (the subagent's own statement, so indicative only).
- The log is written to the main checkout's `.superpowers/factory/events.jsonl`, even when the hook runs inside a worktree.
- The `SubagentHandback` tool shows up as a `PostToolUse` event with the agent's id. Harmless, but it counts as a tool call in the live view.
