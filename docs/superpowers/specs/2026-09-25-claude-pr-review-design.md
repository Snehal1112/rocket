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
- One source of truth for "what good Rocket code looks like" — `CLAUDE.md`
  and `.claude/rules/*.md`, already used by both this CI job and human/agent
  contributors — rather than a second, hand-maintained copy of the rules.
  The two currently-orphaned `.github/agents/*.md` files stay unreferenced
  either way.

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

- Trigger: `pull_request: types: [opened, synchronize, ready_for_review,
  reopened]`, `branches: [main]`.
- Belt-and-suspenders skip: `if: github.event.pull_request.draft == false`
  (the `code-review` skill below also skips drafts on its own judgment, but
  an explicit workflow-level guard avoids spending a runner minute finding
  that out).
- Uses `anthropics/claude-code-action@v1` (pinned major version, not
  `@main`), authenticating with `claude_code_oauth_token` (see secret below).
  No `mode` input — the action infers automation mode from the presence of a
  `prompt` input (verified against `code.claude.com/docs/en/github-actions`,
  current as of this spec).
- `permissions: { contents: read, pull-requests: read, issues: read,
  id-token: write }` — matches Anthropic's own published review-workflow
  example. `id-token: write` is required for the action's default GitHub App
  authentication; posting comments happens through the App's own installed
  permissions via an MCP tool call, not the workflow's ambient
  `GITHUB_TOKEN`, so `pull-requests: write` isn't needed here.
- Checkout with `fetch-depth: 1` (shallow) — a PR review needs the diff, not
  full history; matches the verified official example.

### Review prompt: Anthropic's `code-review` plugin, not a hand-rolled one

`claude-code-action` runs Claude Code itself inside the runner, and Claude
Code always loads the repository's `CLAUDE.md` as project context — the same
mechanism that surfaces `CLAUDE.md`'s Hard Rules and the `.claude/rules/`
pointer chain to a human working in this repo applies automatically to the
action's run, once the workflow checks the repository out. So instead of
hand-duplicating those rules into a new custom prompt (this spec's original
plan), the workflow invokes Anthropic's own `code-review` plugin skill:

```yaml
plugin_marketplaces: "https://github.com/anthropics/claude-code.git"
plugins: "code-review@claude-code-plugins"
prompt: "/code-review:code-review --comment ${{ github.repository }}/pull/${{ github.event.pull_request.number }}"
claude_args: '--allowedTools "mcp__github_inline_comment__create_inline_comment"'
```

`--comment` posts findings as inline PR comments, or one summary comment
when it finds none; without it, results only land in the workflow run log.
The `claude_args` line is required even though the skill's own frontmatter
names the same tool, because the action only starts the MCP server that
posts inline comments when `--allowedTools` names it in `claude_args`.

This keeps `CLAUDE.md` and `.claude/rules/*.md` as the single source of
truth for "what good Rocket code looks like" — no second copy of the rules
to keep in sync — and leaves the two orphaned `.github/agents/*.md` files
unreferenced (deleting them is out of scope here; a separate cleanup if
ever needed).

### `CLAUDE_CODE_OAUTH_TOKEN` secret

Authenticated via a Claude Pro/Max subscription, not a pay-per-token
Anthropic API key: generate a token locally with `claude setup-token`
(requires the Claude GitHub App to be installed on the repo at
https://github.com/apps/claude first), then add it as a repo secret
(Settings → Secrets and variables → Actions → `CLAUDE_CODE_OAUTH_TOKEN`).
The workflow passes it as the action's `claude_code_oauth_token` input
instead of `anthropic_api_key` — the two inputs are mutually exclusive, and
if both were set the API key would silently take precedence, so
`anthropic_api_key` must not be set at all.

**Manual setup step, not automated by this change** — provisioning
credentials is outside what should happen unattended. Documented as a
prerequisite in the implementation plan.

**Caveat to verify during implementation:** the OAuth token is a personal
credential tied to your subscription (not a service account), and unlike an
API key it has no rotation/revocation UI in the Anthropic console — if it
expires or is revoked, `claude setup-token` needs to be re-run and the secret
updated manually. Also, `claude-code-action`'s `classify_inline_comments`
feature requires `anthropic_api_key` and is skipped under OAuth — not used
by this design, but worth knowing if a later sub-project wants it.

## Data Flow

1. PR opened, pushed to, reopened, or marked ready for review.
2. GitHub triggers `claude-review.yml` in parallel with `pr-check.yml`.
3. Shallow checkout → `claude-code-action` runs the `code-review` skill
   against the PR diff, with `CLAUDE.md`/`.claude/rules/*.md` loaded
   automatically as project context.
4. Action posts inline PR comments (or one summary comment when clean) via
   the GitHub App's own permissions, and skips draft/closed/already-reviewed
   PRs on its own judgment in addition to this workflow's explicit draft
   guard.

## Error Handling & Cost Control

- Missing/invalid/expired `CLAUDE_CODE_OAUTH_TOKEN` → this job fails visibly
  in the Actions tab; `pr-check.yml` is unaffected and merge is still
  possible.
- Draft PRs are skipped entirely (no review run, no cost).
- Action pinned to a released version so behavior doesn't shift under us
  without an explicit version bump.
- Scoped to `pull_request` → `main` only — no runs on arbitrary branch pushes.

## Security Considerations

- Trigger is plain `pull_request`, never `pull_request_target`: GitHub
  itself withholds secrets from workflow runs triggered by fork pull
  requests under `pull_request`, so a fork PR simply can't reach
  `CLAUDE_CODE_OAUTH_TOKEN` or trigger a review run at all — confirmed
  against Anthropic's own docs, not just an assumption. `Snehal1112/rocket`
  is a personal repo with no external contributors today, but this holds
  regardless.
- `permissions` block is scoped to the minimum needed for the verified
  official pattern (`contents: read`, `pull-requests: read`, `issues: read`,
  `id-token: write`) rather than defaulting to broader repo-token
  permissions.

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
5. Push a follow-up commit to the same PR and observe what happens — official
   docs state the skill "skips... pull requests that already have a comment
   from Claude," which reads as skip-on-repeat rather than update-in-place,
   but this spec doesn't rely on either behavior being true. The only
   required property is no duplicate-comment spam; record which behavior is
   actually observed for future reference.

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

- The Claude GitHub App must be installed on the repo and
  `CLAUDE_CODE_OAUTH_TOKEN` generated (`claude setup-token`) and added as a
  repo secret manually before this workflow can run (see Components above).
- Because the OAuth token rides on a personal Claude subscription, review
  runs are attributed to and rate-limited against that individual's plan —
  worth knowing if usage volume grows enough to matter.
