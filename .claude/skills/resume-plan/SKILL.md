---
name: resume-plan
description: Use after /clear or context compaction to find where multi-plan work stands, what is done, and the single next action.
---

# Resume Plan

Trust the ledger and git over recollection. Work through this in order.

1. Read the memory index (`MEMORY.md`) and open only the project entries relevant to this repo, such as plan series and branch status notes.
2. List `.superpowers/sdd/*/progress.md`. Read the tail of each ledger. A task with a "Task N: complete" line is DONE. Never re-dispatch it.
3. Run `git status` and `git log --oneline -15`. Compare with the ledger. Note commits the ledger does not mention and ledger claims with no commit.
4. Call TaskList and note open and in-progress tasks.
5. Report the state per plan (done, in progress, not started) and the single next action.

## Rules

- Never run more than one implementer in a worktree at a time.
- If the ledger and git disagree, say so and prefer git for what exists and the ledger for review rulings.
- Do not start work before reporting the state.
