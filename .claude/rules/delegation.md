# Delegation (decompose and dispatch)

Use the `decompose-and-dispatch` skill when an ask mixes sub-steps of different difficulty (lookups, mechanical edits, tricky design) and no narrower skill fits.

## Check first

- Trivial ask (one or two tool calls, no ambiguity): do it directly.
- A narrower skill already fits: use that instead.

| Ask shape | Use |
|---|---|
| Executing a written implementation plan | `superpowers:subagent-driven-development` |
| Two or more independent failures or subsystems | `superpowers:dispatching-parallel-agents` |
| Open-ended investigation or root cause | `decomposing-investigations` |
| Clearing a `cargo clippy` backlog | `fixing-clippy-warnings` |

- Tightly coupled sub-steps (they must agree on one rule, format or invariant) stay in one thread, even across many files.

## Model tiers

| Tier | Model | Use for |
|---|---|---|
| Cheapest | haiku | Lookups, running lint or tests, applying an already-decided small edit |
| Standard | sonnet | Multi-file tracing, debugging, implementing from a clear spec |
| Strongest | opus or the main loop | Design, ambiguous debugging, security review, anything touching an invariant |

- Always state the model in the dispatch. An omitted model inherits the session model, usually the most expensive one.

## Dispatch rules for Rocket

- A subagent starts with no session context. Quote the relevant rule files, ledger notes and brief paths into its prompt.
- Independent sub-steps go out in one message. Dependent ones run in sequence.
- One implementer at a time per worktree. Read-only reviewers and investigators can run alongside it.
- Subagents commit with a pathspec commit (`git add <paths> && git commit --only -m "..." -- <paths>`), never `git add -A`.
- Read each subagent's actual output and diff before trusting its summary.
- Record non-obvious findings and decisions in the plan ledger (see [harness.md](harness.md)).
