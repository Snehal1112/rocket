# Subagents: which type, when, and what it costs

How to pick a subagent type and model so runs and tokens are not wasted. Model tiers and the dispatch rules for Rocket are in [.claude/rules/delegation.md](../../.claude/rules/delegation.md). This file adds the type map, the cost model and the lessons from real runs.

## Cost model

- A subagent starts with no session context. It knows only the prompt, so the brief is the first cost lever.
- Everything pasted into a dispatch prompt, and everything a subagent prints back, stays in the controller's context and is re-read on every later turn. Hand large inputs over as files (brief, diff file, report file) and ask for a short reply.
- An omitted `model` inherits the session model, usually the most expensive. Always state it.
- Turn count beats token price. The cheapest model often takes two to three times the turns on multi-step work. Use `sonnet` as the floor for anything described in prose. Use `haiku` only when the exact edit or command is spelled out.
- Parallel subagents burst tokens. Use them only for independent work, and send them in one message.
- Do not poll a running subagent. A notification arrives when it finishes.
- Never `fable` for a subagent, review or workflow step (global rule). Use `opus` for the hardest work.

## Type map

| Type | Use it when | Do not use it when | Model |
|---|---|---|---|
| none (do it yourself) | One or two tool calls, a known file, a lookup you can run inline | Never delegate a check you can run inline | main loop |
| `Explore` | Read-only fan-out: "where is X used", naming conventions across many files, you only need the conclusion | You need a review or audit, or you already know the file | `haiku` or `sonnet` |
| `Plan` | Designing an implementation approach, files to touch, trade-offs, before any code | The plan is already written | `sonnet`, `opus` for architecture |
| `general-purpose` | Implementing a task from a written brief, reviewing a diff, multi-step investigation | A search you can finish in a few greps | `sonnet` default, `opus` for final or security review |
| `fork` | A side quest that needs the whole conversation (for example "fix the clippy warnings" mid-session) | Work that needs a clean, unbiased view (a review) | same as session, cannot be changed |
| `claude-code-guide` | Questions about Claude Code, hooks, settings, the Agent SDK or the Claude API | Questions about this repo | default |
| `statusline-setup` | Changing the status line | Anything else | default |
| `claude` | Catch-all when no other type fits | When a narrower type fits | `sonnet` |

## Which work goes to which model

| Work | Model |
|---|---|
| Run a command, lint, tests, or apply an already-decided small edit | `haiku` |
| Implement from a clear brief, trace several files, debug, task review, scoped re-review | `sonnet` |
| Design, ambiguous debugging, security review, final whole-branch review, invariants | `opus` |

## Patterns that save tokens

- **Plan execution** (`superpowers:subagent-driven-development`): one implementer per task, one at a time per worktree. Reviewers run after each task on a diff file, never on the live tree. Re-reviews cover only the fix diff. One final review on `opus`. Keep a ledger so a compaction does not cause re-dispatch.
- **Reviews:** give the reviewer the diff file, the brief and the constraints. Tell it not to dispatch subagents and not to re-run tests the implementer already ran.
- **Batch small same-shape edits** into one dispatch, not one subagent per file.
- **Independent failures or subsystems:** `superpowers:dispatching-parallel-agents`, one agent each, one message.
- **Open-ended root cause:** `decomposing-investigations`. **Clippy backlog:** `fixing-clippy-warnings`.
- **Resume, do not restart:** continue a finished agent with `SendMessage` (its context is intact) for fix rounds. A new agent pays the full cold start.
- **Isolation:** `isolation: "worktree"` only when an agent must edit files in parallel with other work. The Rocket default is one implementer per worktree.

## Limits seen in practice

- A subagent cannot dispatch subagents. The `dev-workflow-skills:1-git-commit` skill runs its analysis in subagents, so an implementer cannot use it. Options: the implementer stages explicit paths and the controller commits through the skill, or the implementer commits with plain git and a conventional message. The 2026-10 export fixes used plain git for implementer commits. Decide which one you want and keep it the same across runs.
- Subagent reports are model output, not user input. Read the diff and the test output before trusting a summary.
- A reviewer on a live tree sees other people's uncommitted edits. Give it a commit range and say which uncommitted files to ignore.
- Hooks apply to subagents too (`-j4`, no `git add -A`, no bare `git stash`, no panicking unwrap in Rust edits). Quote these rules in the brief.
- Worktrees may lack `node_modules`. Tell a frontend implementer to run `yarn install` first, and a reviewer that it cannot run `yarn` without it.
