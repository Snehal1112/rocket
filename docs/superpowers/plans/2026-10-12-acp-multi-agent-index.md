# ACP Multi-Agent Support: Plan Index

Spec and verified facts: `docs/features/acp-multi-agent-support.md`.

**Goal:** the workspace assistant works with Gemini CLI and, later, other ACP agents, through an `AgentKind` enum and one pure `session_plan` function, and its reasoning is visible, controllable and used to guide and correct the user on an endpoint. Claude behavior does not change except where a plan says so.

Each plan has at most 3 tasks. They run strictly in order, one at a time, in the same worktree. After a plan finishes with green checks, my Claude starts the next one without asking again.

| Order | Plan | Tasks | Result |
|---|---|---|---|
| 1 | [Plan 1: AgentKind plumbing](2026-10-12-acp-multi-agent-plan-01-agent-kind.md) | 1. `AgentKind` on config, DTO, dialog. 2. Pure `session_plan` and the service that uses it. 3. Scratch file writer and command wiring. | Claude and Generic work through the plan. Nothing visible changes for Claude. |
| 2 | [Plan 2: Gemini preset](2026-10-12-acp-multi-agent-plan-02-gemini-preset.md) | 4. Gemini arm, reserved env, tool-name parity test. 5. Gemini in the dialog. 6. Real-key verification (manual gate). | Gemini sees only the Rocket tools and can propose changes. |
| 3 | [Plan 3: Hardening and docs](2026-10-12-acp-multi-agent-plan-03-hardening.md) | 7. Stdio bridge tools capability. 8. Request docs in the tool description, docs updates. | Gaps from the first investigation closed. |
| 4 | [Plan 4: Reasoning visibility and liveness](2026-10-12-acp-multi-agent-plan-04-reasoning-visibility.md) | 9. Forward thought chunks (and count them as activity). 10. Reasoning block and quiet-agent notice. 11. Configurable idle limit per agent. | The user sees the reasoning, and a long think no longer kills the session. |
| 5 | [Plan 5: Reasoning level and Gemini model](2026-10-12-acp-multi-agent-plan-05-reasoning-level.md) | 12. `reasoning` and `model` on the agent tuning. 13. Apply to Claude effort and Gemini thinking. 14. Dialog and composer controls, and a verification run. | One Reasoning setting (default High) drives Claude and Gemini. Gemini model can be pinned. |
| 6 | [Plan 6: Guidance quality](2026-10-12-acp-multi-agent-plan-06-guidance-quality.md) | 15. `get_request` returns docs. 16. Review docs proposals as Markdown. 17. Guide-and-correct instructions, `/review` and `/howto`. | The assistant reads the docs, explains what is wrong, writes good docs, and says what live testing needs. |
| 7 | [Permission card](2026-10-12-acp-permission-card.md) (existing, 7 tasks) | see the plan | Generic agents can ask the user for permission. Re-read its Tasks 1 and 3 first, because it predates Plans 1 to 6. |
| 8+ | One plan per further agent, written when needed | | Qwen, Codex, OpenCode, Goose, Copilot, Cursor. |

## Reasoning gaps and where each is covered

| Gap found by the investigation | Covered by |
|---|---|
| Thought chunks dropped, never shown | Task 9 (forward), Task 10 (Reasoning block) |
| Long silent thinking killed by the 120 s idle limit | Task 9 (thoughts count as activity), Task 11 (limit per agent), Task 10 (quiet-agent notice) |
| Rocket never sets or remembers effort | Task 13 (applied at start), Task 13 and 14 (remembered per agent) |
| Gemini has no effort option and no model picker | Task 12 and 13 (thinking and model written into Gemini settings), Task 14 (dialog field) |
| Agent cannot read a request's docs | Task 15 |
| Docs proposals shown as JSON-escaped text | Task 16 |
| Stored messages cut at 16 KiB | Decided to keep (ruling in Plan 6): proposals apply from the card, not from chat text |
| Trying an endpoint needs Agent mode, run switch and trust | Task 17 (the assistant explains it and asks for the last response), unchanged on purpose for security |
| No response history, 8 KB response cap | Task 17 (user can attach the last response with `#`), cap unchanged on purpose |
| Claude often sends empty thinking text | Not fixed here (adapter behavior). Task 14 step 11 records it, followups list a summarized-thinking experiment |

## Reasoning findings (2026-10-12, read from source and docs, not run live)

- Claude (claude-agent-acp): thinks adaptively by default, cannot be switched off on the newest models. Exposes `model`, `effort` and `mode` options. Streams thinking as `agent_thought_chunk`, but recent models often send empty text. Rocket's `settingSources: []` override works (the adapter spreads the user options after its default, `acp-agent.ts` lines 9991 to 9993).
- Gemini 0.63.0: thinks by default (`includeThoughts` on). Gemini 3 models default to a high thinking level, 2.5 models to a budget of 8192. It streams thoughts as `agent_thought_chunk`. ACP clients get only modes and models, no effort option, and `session/set_config_option` is not implemented. Thinking is set only through `modelConfigs` in the settings file.
- Rocket before Plan 4 dropped every thought chunk, so thinking neither reached the UI nor restarted the idle timer.

## Chain

Plan 1, 2, 3, 4, 5, 6, then the permission card, then agent-specific plans. Each plan file ends with a "Next plan" section naming the following plan.

Gates that stop the chain:

- Any plan's final checks red: fix before the next plan.
- Plan 2 Task 6 fails (a built-in tool appears, or Google rejects the `propose_changes` schema): write a fix plan before Plan 3. Task 6 needs a real `GEMINI_API_KEY` and a person at the app. My Claude stops there and asks.
- Plan 5 Task 14 step 10 fails (the Gemini thinking override does not reach the API): fix the override shape before Plan 6.

## Decisions (rulings)

- Ruling: closed `AgentKind` enum with Rust presets plus a pure `session_plan`. No trait, and not data-driven templates (they cannot express credential copying, version flags, schema transforms or isolation checks, and they widen the attack surface).
- Ruling: Gemini is locked down through exact-name `tools.core` in the user settings of a private `GEMINI_CLI_HOME`. The system settings file is ignored unless root-owned, and a wildcard allow-list denies every MCP call. Exact names also skip the permission prompt, so Gemini does not need the permission card.
- Ruling: the permission card is kept, but moved after Gemini and the reasoning plans, and used for Generic agents.
- Ruling: no Google OAuth in v1. Gemini uses `GEMINI_API_KEY`.
- Ruling: Gemini is selectable in the dialog only from Plan 2 on. Plan 1 ships Claude and Generic.
- Ruling: reasoning is one Rocket setting (`low`, `medium`, `high`, default `high`) mapped to each agent's own control. The default raises Claude's effort above the adapter default on the newest models, at a cost in tokens and time. The user can lower it per agent.
- Ruling: thought text is shown to the user only. It is not stored in conversations and not sent back to agents.
- Ruling: new per-agent knobs go in `AgentConfig.tuning`, so later fields do not touch every `AgentConfig` literal again.

## Execution notes

- One implementer per task, one at a time per worktree. A separate reviewer reviews a diff file, not the live tree. Run a final whole-branch review before merge. See `.claude/rules/harness.md`.
- Keep a ledger per plan at `.superpowers/sdd/<plan>/progress.md` and log decisions as `Ruling:` lines. After `/clear`, use the `resume-plan` skill.
- Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Subagents commit through `dev-workflow-skills:1-git-commit` with a pathspec commit, never `git add -A`.
- Model tiers: Sonnet for implementers and per-plan reviews, Haiku for mechanical edits, Opus for the final review only. State the model in every dispatch.
- A project hook blocks the literal call-syntax for unwrap in any Rust-related content. Write plan text without it.
- Nothing is pushed. Merge to local main only.
