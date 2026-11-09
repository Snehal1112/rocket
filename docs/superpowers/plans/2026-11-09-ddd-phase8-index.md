# DDD Phase 8 (contexts and crate structure): plan index

Roadmap: `docs/roadmaps/ddd-roadmap.md` Phase 8 (D-80 to D-88). One spec per series, one plan per row of a series, at most three tasks each, run in a git worktree, merged to local `main` only after a fresh `opus` review. Series 1 executes after the uncommitted team-network work in `rocket-app` is committed (or rebase the worktree on it first).

| Series | Items | Spec | Plans | Status |
|---|---|---|---|---|
| 1 | D-83 `AssistantMode` to `rocket-acp`, D-84 script cookie API to `rocket-scripting`, D-87 glossary, D-88 `Template` to `rocket-collection` | `docs/superpowers/specs/2026-11-09-ddd-phase8-series1-small-moves-design.md` | `2026-11-09-ddd-phase8-series1-plan-1-small-moves.md` (3 tasks) | done (2026-11-09) on branch worktree-ddd-phase8-series1, final review clean |
| 2 | D-86 pure parts (`graphql_document`, `assertion_evaluator`, `effective_capabilities`), D-82 contract, conversation and proposal rules (includes merging `ConversationMode` into `AssistantMode`) | `docs/superpowers/specs/2026-11-09-ddd-phase8-series2-domain-rules-design.md` | `2026-11-09-ddd-phase8-series2-plan-1-pure-parts.md`, `...-plan-2-contract-conversation.md`, `...-plan-3-proposal-rules.md` (3 tasks each, run in that order) | done (2026-11-09) on branch worktree-ddd-phase8-series1 |
| 3 | D-81 OpenAPI sync home (new crate `rocket-openapi`) | `docs/superpowers/specs/2026-11-09-ddd-phase8-series3-openapi-sync-home-design.md` | `2026-11-09-ddd-phase8-series3-plan-1-foundations.md` (2 tasks), `...-plan-2-service-move.md` (3 tasks), run in that order | approved, running |
| 4 | D-80 flow runtime extraction | not written | not written | after series 3, likely several plans |
| note | D-85 `rocket-http` split | decision note only | none | roadmap says "consider" |
