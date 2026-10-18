# Token A/B test: #75 through the factory, #64 inline

Run 2026-10-18. Question: does building a small ticket directly with superpowers skills cost about the same as the factory? Background: the investigation of jobs 021-024 (memory note `project_factory_token_investigation_issue22`).

#63 was planned as the inline ticket and swapped for #64 (same size S, crate-local). #63 is in `src-tauri`, and a cold build of that package builds the whole app including V8, which stalled a factory builder before. The disk was 93% full.

## Method

- Factory run: `/factory` for #75 (rocket-http small helpers, size S), job 025. Tokens come from `usage.jsonl`, weighted as input + cache_creation + 0.1 * cache_read + output.
- Inline run: #64 (rocket-app small helpers, size S). A plan in the same style as the #75 plan was written first. It was executed in a headless `claude -p` session (Sonnet, one context, no subagents, no commits) in the worktree `/home/numericlabs/data/rocket/inline-64` on branch `inline-64`. The JSON usage of that session gives the same weighted figure.
- The session counter of the interactive session reset mid-way and could not be used.

## Results

| | Factory #75 (job 025) | Inline #64 |
|---|---|---|
| Weighted tokens, total | 322.6K | 266.2K |
| Of which building | 206.5K (3 builder runs, 17 turns) | 266.2K (1 run, 53 turns) |
| Of which committer, reviewer, message | 116.2K | none (no commit, review or message step) |
| Fixed context loaded at run start | 171.5K over 6 runs (53%) | 88.3K cache creation (one run) |
| Cache reads at 0.1 | not split here | 165.6K (62%) |
| Output | about 12K | 12.2K |
| Diff | 7 files, +474/-30 (golden tests and refactor) | 2 files, +153/-25 |
| Wall time | not measured | 23.8 minutes (more than 10 minutes was the cold build) |
| Review | code review passed | none; tests re-run independently (39 and 56 passed, `cargo check` clean) |
| Cost in USD (list prices) | not available | 0.64 |

Weighted figure of the inline run: 48 + 88,334 + 0.1 * 1,655,992 + 12,202 = 266,183.

## Reading the numbers

- Like for like (building only), the factory builders cost 206.5K for a larger diff and the inline run cost 266.2K for a smaller one. The inline run was about 29% more, on a smaller change.
- The inline run avoided the committer, the reviewer and the message agent (116K in the factory run), but it also had no review. Adding those three would give about 382K against the factory's 322.6K.
- The inline run paid its fixed context once, but its long single context (53 turns, about 31K of context re-read each turn) cost 165.6K in cache reads. The factory pays the fixed context six times, but each builder run is short (4 to 7 turns).
- Authoring cost is not in either figure. The #75 interview, spec and plan were written in the interactive session, and the #64 plan was too.

## Limits

- Two different tickets, one run each. This is one data point, not proof.
- The #75 plan gave full test code and a recording pattern. The #64 plan asked the agent to derive invalid status and event pairs from the state machine, which is more exploration. That alone could explain part of the 53 turns.
- The #64 run included a cold `rocket-app` test build (more than 10 minutes), which adds waiting turns.
- The inline summary reported a fifth `transition_status` call (in `recompute_drift_for_collection`) that the plan did not list, and left it alone. The plan also said five lock sites in one place and four in another. The ticket says five, and one of them is the lenient rollback that must stay.

## Cleanup

Done 2026-10-18. The #64 change was committed as `1a988d88` (`refactor(app): share lock and transition mapping`, `Relates to: #64`) and fast-forwarded to local `main`, not pushed. The worktree `inline-64`, the branch `inline-64`, the untracked plan and the result files were removed. #64 is still open on GitHub.
