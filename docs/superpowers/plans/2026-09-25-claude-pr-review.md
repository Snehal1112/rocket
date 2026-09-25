# Claude PR Review (Advisory) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a standalone, advisory-only GitHub Actions workflow that runs Claude's `code-review` skill on every non-draft PR against `main`, using the repo's own `CLAUDE.md`/`.claude/rules/*.md` as its review criteria.

**Architecture:** One new workflow file, `.github/workflows/claude-review.yml`, running `anthropics/claude-code-action@v1` in a job separate from `pr-check.yml` so it can never block a merge. No application code changes.

**Tech Stack:** GitHub Actions, `anthropics/claude-code-action@v1`, Anthropic's `code-review` plugin skill, `CLAUDE_CODE_OAUTH_TOKEN`.

**Spec:** `docs/superpowers/specs/2026-09-25-claude-pr-review-design.md`

## Prerequisites (manual, human — not a task below)

Before Task 2 can produce a live signal:

1. Install the [Claude GitHub App](https://github.com/apps/claude) on `Snehal1112/rocket`.
2. Run `claude setup-token` locally (requires a Claude Pro/Max subscription) to mint a long-lived OAuth token.
3. Add it as a repository secret named `CLAUDE_CODE_OAUTH_TOKEN` (Settings → Secrets and variables → Actions).

Task 1 does not require this — the workflow file can be written and structurally validated without a live secret. If the secret isn't set yet when Task 2 runs, the job will fail visibly with an auth error instead of posting a review; that's still a valid, non-blocking outcome to record (see Task 2, Step 3's fork).

## Global Constraints

- Advisory only: this workflow's outcome must never block or delay a merge. It stays in its own file, never a step inside `pr-check.yml`.
- Authenticate with `claude_code_oauth_token` (`CLAUDE_CODE_OAUTH_TOKEN` secret) — never `anthropic_api_key`.
- Pin the action to `anthropics/claude-code-action@v1`, never `@main`.
- Trigger on plain `pull_request` — never `pull_request_target`.
- `permissions: { contents: read, pull-requests: read, issues: read, id-token: write }` exactly — no broader scope.
- `fetch-depth: 1` on checkout — no full-history clone.
- Do not hand-write review rules into a prompt — rely on `claude-code-action` auto-loading `CLAUDE.md` (and, through it, `.claude/rules/*.md`) plus Anthropic's `code-review` plugin skill.
- Do not modify `pr-check.yml`, `build.yml`, or `release.yml`.
- Every commit in this plan goes through the `dev-workflow-skills:1-git-commit` skill, not a freeform `git commit -m`.

## Review Focus

- **Draft PR opened** — expect no review run (job skipped, not just non-posting); covered by Task 1's static check and exercised live in Task 2.
- **PR targets a branch other than `main`** — expect the workflow not to trigger at all; covered by Task 1's static check on the `branches:` filter.
- **Fork PR** — GitHub itself withholds secrets from `pull_request`-triggered runs on fork PRs; covered by Task 1's static check that `pull_request_target` never appears in the file.
- **Rapid successive pushes to the same PR** — expect the `concurrency` group to cancel an in-flight run rather than piling up duplicate/overlapping runs; covered by Task 1's static check for a `concurrency:` block keyed on the PR number.
- **Missing/expired `CLAUDE_CODE_OAUTH_TOKEN`** — expect this job to fail visibly in the Actions tab without affecting `pr-check.yml` or mergeability; covered by Task 2's fork on whether the secret is provisioned yet.

---

### Task 1: Add and structurally validate the workflow file

**Files:**
- Create: `.github/workflows/claude-review.yml`
- Test: none (no workflow-testing framework in this repo) — validated with the inline structural check below, run before and after the file exists.

**Interfaces:**
- Consumes: nothing from earlier tasks (first task).
- Produces: `.github/workflows/claude-review.yml`, referenced by Task 2 as the workflow under test. Depends on the repo secret `CLAUDE_CODE_OAUTH_TOKEN` existing at runtime (see Prerequisites) — not required to exist for this task's own checks to pass.

- [ ] **Step 1: Write the structural check and confirm it fails (file doesn't exist yet)**

Run this exact check:

```bash
FILE=.github/workflows/claude-review.yml
set -e
test -f "$FILE"
python3 -c "import yaml; yaml.safe_load(open('$FILE'))"
grep -q "anthropics/claude-code-action@v1" "$FILE"
grep -q "claude_code_oauth_token" "$FILE"
grep -q "id-token: write" "$FILE"
grep -q "fetch-depth: 1" "$FILE"
grep -q "branches: \[main\]" "$FILE"
grep -q "if: github.event.pull_request.draft == false" "$FILE"
grep -q "concurrency:" "$FILE"
! grep -q "pull_request_target" "$FILE"
echo "All checks passed"
```

Expected: FAIL at `test -f "$FILE"` — the file doesn't exist yet.

- [ ] **Step 2: Confirm the failure**

Confirm the command above exited non-zero and did not print "All checks passed". This is the RED step — there's no separate test runner to invoke here, the check script above is both the test and its own runner.

- [ ] **Step 3: Write the workflow file**

Create `.github/workflows/claude-review.yml` with exactly this content:

```yaml
name: Claude Code Review

on:
  pull_request:
    types: [opened, synchronize, ready_for_review, reopened]
    branches: [main]

concurrency:
  group: claude-review-${{ github.event.pull_request.number }}
  cancel-in-progress: true

jobs:
  review:
    if: github.event.pull_request.draft == false
    runs-on: ubuntu-22.04
    permissions:
      contents: read
      pull-requests: read
      issues: read
      id-token: write
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 1

      - name: Claude Code Review
        uses: anthropics/claude-code-action@v1
        with:
          claude_code_oauth_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
          plugin_marketplaces: "https://github.com/anthropics/claude-code.git"
          plugins: "code-review@claude-code-plugins"
          prompt: "/code-review:code-review --comment ${{ github.repository }}/pull/${{ github.event.pull_request.number }}"
          claude_args: '--allowedTools "mcp__github_inline_comment__create_inline_comment"'
```

Notes for the engineer:
- `runs-on: ubuntu-22.04` and `actions/checkout@v4` match `pr-check.yml`/`build.yml`'s existing convention in this repo — do not "upgrade" these to match Anthropic's own docs examples (which use `ubuntu-latest`/`checkout@v6`); consistency with this repo's other workflows wins.
- The `concurrency` block is modeled on `pr-check.yml`'s own `concurrency` block, scoped to the PR number instead of the branch ref since this workflow can run on PRs from any branch.

- [ ] **Step 4: Re-run the structural check and confirm it passes**

Run the same command from Step 1 again.
Expected: prints `All checks passed`, no non-zero exit.

- [ ] **Step 5: Commit**

Stage `.github/workflows/claude-review.yml`, then invoke the `dev-workflow-skills:1-git-commit` skill to draft and create the commit (per project convention — do not run a freeform `git commit -m`).

---

### Task 2: Live smoke test on a throwaway PR

**Files:**
- Create (temporary, deleted at the end of this task): `src/components/__smoketest__/SmokeTestButton.tsx`

**Interfaces:**
- Consumes: `.github/workflows/claude-review.yml` from Task 1, already merged to `main` (the `pull_request` trigger only fires for workflow files present on the base branch).
- Produces: a recorded observation (in this task's own notes/PR description, not a new doc) of whether `claude-review.yml` updates or skips its comment on a follow-up push — resolves the open question the spec flagged in its Verification section. No other later task depends on this task's output; this plan has no Task 3.

- [ ] **Step 1: Confirm Task 1 is on `main`**

Run: `git log --oneline -1 -- .github/workflows/claude-review.yml`
Expected: shows the commit from Task 1, and `git branch --show-current` shows `main` (or the workflow file's commit is already merged to `main` if working from elsewhere).

- [ ] **Step 2: Create the throwaway branch and violation**

```bash
git checkout -b chore/smoke-test-claude-review
mkdir -p src/components/__smoketest__
cat > src/components/__smoketest__/SmokeTestButton.tsx <<'EOF'
// Deliberate Hard Rule violation for the claude-review.yml smoke test.
// Raw <button> instead of shadcn/ui Button — delete this file after the
// smoke test in docs/superpowers/plans/2026-09-25-claude-pr-review.md
// Task 2 is complete.
export function SmokeTestButton() {
  return <button onClick={() => {}}>Click me</button>
}
EOF
git add src/components/__smoketest__/SmokeTestButton.tsx
```

Then invoke the `dev-workflow-skills:1-git-commit` skill to commit this throwaway file (it will be removed in Step 7 — a real, if short-lived, commit is fine here since the whole branch is discarded).

- [ ] **Step 3: Open the PR as a draft and confirm the review is skipped**

Push the branch and open a **draft** PR against `main` (e.g. `gh pr create --draft --title "chore: smoke test claude-review.yml" --body "Throwaway PR for docs/superpowers/plans/2026-09-25-claude-pr-review.md Task 2. Do not merge."`).

Expected: in the PR's checks, the `Claude Code Review` job shows as **skipped** (its `if: github.event.pull_request.draft == false` condition is false) — not absent, not run.

- [ ] **Step 4: Mark ready for review and confirm the job runs**

Run: `gh pr ready` on the PR (or use the GitHub UI "Ready for review" button).

Expected: the `Claude Code Review` job now runs (triggered by `ready_for_review`). One of two outcomes is acceptable, and either resolves the Prerequisites fork above — record which one happened:
- **Secret provisioned:** the job succeeds and posts a review comment flagging the raw `<button>` (an inline comment, or one summary comment, per the `code-review` skill's own behavior).
- **Secret not yet provisioned:** the job fails visibly in the Actions tab with an authentication error.

In both cases, confirm `pr-check.yml`'s checks pass independently (the file is valid TSX) and the PR's merge button stays enabled — `claude-review.yml`'s outcome must not affect mergeability either way.

- [ ] **Step 5: Push a follow-up commit and observe repeat behavior**

```bash
git commit --allow-empty -m "chore: trigger claude-review.yml re-run for smoke test"
git push
```

Expected: record what `claude-review.yml` actually does — re-runs and updates/adds a comment, or skips because a comment already exists (see spec's Verification section — no behavior is assumed in advance, just observed and noted for future reference).

- [ ] **Step 6: Clean up**

```bash
gh pr close chore/smoke-test-claude-review --delete-branch
git checkout main
git branch -D chore/smoke-test-claude-review 2>/dev/null || true
```

Expected: the throwaway PR is closed without merging, and both the remote and local `chore/smoke-test-claude-review` branches are gone. `git status` on `main` shows no leftover `SmokeTestButton.tsx` (it only ever existed on the deleted branch).

- [ ] **Step 7: Report the observation**

No commit here — this step is reporting only. Summarize for your human partner: whether the secret was provisioned, what the review comment looked like (if any), and whether the follow-up push updated or skipped the review. This closes out the spec's open Verification question; no further plan or task follows from it.
