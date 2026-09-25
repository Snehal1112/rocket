# Claude PR Review (Advisory) — Design Spec

**Status:** Draft for review
**Feature:** Automated Claude-based code review on pull requests
**Part of:** Claude SDLC roadmap, sub-project 1 of 3 (see [Roadmap](#roadmap) below)

## Problem

Rocket's day-to-day development already runs a Claude/Superpowers-driven SDLC —
brainstorm → spec → plan → implement → verify → commit — but that loop is
entirely local and interactive. The CI pipeline (`pr-check.yml`, `build.yml`,
`release.yml`) has no Claude involvement at all: no automated review comments,
and the review checklists already written for Rocket
(`.github/agents/rocket-implementation-reviewer.agent.md`,
`.github/agents/code-reviewer.md`) aren't wired into anything — they're dead
weight today.

This spec covers the first step in closing that gap: an advisory, non-blocking
Claude review that runs automatically on every PR.

## Goals

- Every PR against `main` gets an automated review comment from Claude,
  checking the same rules a human reviewer (or the local `code-review` skill)
  would check: DDD crate boundaries, serde/DTO split, OpenCollection
  compliance, shadcn/lucide-only UI, SingleLineEditor/Monaco split, Zustand
  selector discipline.
- The check runs in parallel with `pr-check.yml` and can never block or delay
  a merge in this phase — it's advisory only.
- One source of truth for "what good Rocket code looks like," usable by both
  this CI job and human contributors, replacing the two currently-orphaned
  `.github/agents/*.md` files.

## Non-goals (this spec)

- Merge-blocking behavior — tracked as sub-project 2, see [Roadmap](#roadmap).
- Release notes generation — sub-project 3.
- Running Claude on `push` to `main`, on draft PRs, or on forks from external
  contributors.
- Replacing or modifying `pr-check.yml`, `build.yml`, or `release.yml`.

## Architecture

A new, standalone workflow, `.github/workflows/claude-review.yml`, triggered
on `pull_request: [opened, synchronize]` targeting `main`. It runs as its own
job in its own file — not a step added to `pr-check.yml` — specifically so a
Claude API outage, rate limit, or bug can never fail the existing
build/typecheck gate. In this phase, `claude-review.yml`'s outcome has no
effect on mergeability.

```
PR opened/updated on branch
        │
        ├──► pr-check.yml   (existing: tsc, cargo check, build)   [blocking]
        │
        └──► claude-review.yml (new: Claude review comment)        [advisory]
```

## Components

### `.github/workflows/claude-review.yml`

- Trigger: `pull_request: types: [opened, synchronize]`, `branches: [main]`.
- Skip condition: `if: github.event.pull_request.draft == false`.
- Uses `anthropics/claude-code-action`, pinned to a specific released version
  tag (not `@main`), `mode: automation` so it runs without needing a
  `@claude` trigger comment.
- `permissions: { contents: read, pull-requests: write }`.
- Checkout with `fetch-depth: 0` so the action has full diff context.

### Consolidated review prompt

A single new prompt, inlined as a multi-line block scalar in the workflow's
`prompt:` input (the action's input surface doesn't include a
prompt-from-file option), that folds together the review criteria currently
scattered across:

- `CLAUDE.md` Hard Rules (shadcn/ui only, lucide-react only, SingleLineEditor
  vs Monaco, Zustand destructuring, no `unwrap()`, no git-CLI shell-outs,
  conventional commits, serde camelCase-on-IPC-DTOs-only).
- `.claude/rules/rust-ddd-boundaries.md`, `tauri-ipc-boundaries.md`,
  `frontend-component-guardrails.md`.
- The checklists in `.github/agents/rocket-implementation-reviewer.agent.md`
  and `code-reviewer.md`.

Those two `.github/agents/*.md` files carry frontmatter (`tools: [read,
search]`, `model: inherit`) shaped for a different agent runtime and aren't
directly consumable by `claude-code-action`'s `prompt` input, so their
substance gets folded into the new prompt rather than referenced in place.
This becomes the one place that defines "what good Rocket code looks like"
for both this CI job and any human/agent reviewer.

### `ANTHROPIC_API_KEY` secret

Required repo secret (Settings → Secrets and variables → Actions). **Manual
setup step, not automated by this change** — provisioning API credentials is
outside what should happen unattended. Documented as a prerequisite in the
implementation plan.

## Data Flow

1. PR opened or pushed to.
2. GitHub triggers `claude-review.yml` in parallel with `pr-check.yml`.
3. Checkout (full history) → `claude-code-action` runs with the consolidated
   prompt against the PR diff.
4. Action posts a PR comment with findings, or updates its existing comment on
   subsequent pushes (not one new comment per push).

## Error Handling & Cost Control

- Missing/invalid `ANTHROPIC_API_KEY` → this job fails visibly in the Actions
  tab; `pr-check.yml` is unaffected and merge is still possible.
- Draft PRs are skipped entirely (no review run, no cost).
- Action pinned to a released version so behavior doesn't shift under us
  without an explicit version bump.
- Scoped to `pull_request` → `main` only — no runs on arbitrary branch pushes.

## Security Considerations

- Repo is a personal/single-owner repo (`Snehal1112/rocket`) today, so
  fork-PR prompt-injection/cost-abuse risk is low, but the workflow should
  still avoid `pull_request_target` (which would expose secrets to
  fork-submitted code) — plain `pull_request` is correct here since it never
  exposes secrets to untrusted fork code.
- `permissions` block is scoped to the minimum needed
  (`contents: read`, `pull-requests: write`) rather than defaulting to
  broader repo-token permissions.

## Verification

CI infrastructure isn't unit-testable in the usual sense. Verification is:

1. Merge this workflow to `main`.
2. Open a throwaway PR with one deliberate rule violation (e.g. a raw
   `<button>` in a React component, or an `unwrap()` in a Rust production
   path).
3. Confirm `claude-review.yml` runs, posts a comment, and flags the violation.
4. Confirm the PR remains mergeable regardless of the review outcome (i.e.
   the job is genuinely advisory, not silently blocking via branch protection
   defaults).
5. Push a follow-up commit to the same PR and confirm the comment updates in
   place rather than duplicating.

## Roadmap

This is sub-project 1 of a 3-part Claude SDLC extension. Noted here for
context; each remaining piece gets its own spec when picked up:

1. **Claude PR review (advisory)** — this spec.
2. **Promote to merge-blocking gate** — once sub-project 1 has run long enough
   to trust its signal-to-noise ratio, extend `claude-review.yml` (or add a
   required status check) so Critical findings block merge.
3. **Claude-enhanced release notes** — `release.yml` currently generates
   `CHANGELOG.md` mechanically via `git-cliff` from conventional commits; add
   a Claude pass that drafts a readable, user-facing summary for the GitHub
   release body, without replacing the mechanical changelog.

## Open Items

- `ANTHROPIC_API_KEY` must be provisioned manually before this workflow can
  run (see Components above).
