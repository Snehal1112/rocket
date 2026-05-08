# SP0 — Worktree Setup

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create an isolated git worktree on a dedicated branch, copy spec + plans into it, verify a clean baseline. All subsequent plans (SP1-SP9) MUST run inside this worktree.

**Architecture:** Uses `git worktree add` to create `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`. The main working tree is untouched until a PR merge.

**Tech Stack:** Git, Bash

**Must complete before:** SP1-01

---

## Task 1: Verify or create `.worktrees/` directory and ensure it is git-ignored

**Files:**
- Possibly modify: `.gitignore`

- [ ] **Step 1: Check for an existing worktree directory**

```bash
cd "$(git rev-parse --show-toplevel)"
ls -d .worktrees 2>/dev/null && echo "exists" || echo "missing"
ls -d worktrees  2>/dev/null && echo "exists" || echo "missing"
```

If `.worktrees/` exists → skip to Step 3.
If `worktrees/` exists → use that path everywhere below instead of `.worktrees/`.
If neither exists → continue to Step 2.

- [ ] **Step 2: Create `.worktrees/` directory**

```bash
mkdir .worktrees
```

- [ ] **Step 3: Verify `.worktrees/` is git-ignored**

```bash
git check-ignore -q .worktrees && echo "✓ ignored" || echo "✗ NOT ignored"
```

If the output is `✗ NOT ignored`:

```bash
echo ".worktrees/" >> .gitignore
git add .gitignore
git commit -m "chore: add .worktrees to gitignore"
```

Re-run the check — must print `✓ ignored` before proceeding.

- [ ] **Step 4: Commit**

Only needed if `.gitignore` was changed (done in Step 3). Otherwise nothing to commit here.

---

## Task 2: Create the worktree + new branch

**Files:** none — git plumbing only

- [ ] **Step 1: Create the worktree on a new branch**

```bash
cd "$(git rev-parse --show-toplevel)"
git worktree add .worktrees/contract-enhancement -b feat/contract-lock-enhancement
```

Expected output (example):

```
Preparing worktree (new branch 'feat/contract-lock-enhancement')
HEAD is now at abc1234 <last commit message>
```

If the branch already exists (re-run scenario):

```bash
git worktree add .worktrees/contract-enhancement feat/contract-lock-enhancement
```

- [ ] **Step 2: Confirm the worktree is listed**

```bash
git worktree list
```

Expected: two entries — the main worktree and `.worktrees/contract-enhancement` on `feat/contract-lock-enhancement`.

- [ ] **Step 3: Copy spec + plans into the worktree**

Run from the **main repo root** (not the worktree):

```bash
WT=".worktrees/contract-enhancement"
mkdir -p "$WT/docs/superpowers/specs" "$WT/docs/superpowers/plans/contract-enhancement"

# Spec
cp docs/superpowers/specs/2026-05-07-contract-lock-enhancement-design.md \
   "$WT/docs/superpowers/specs/"

# Plans
for f in sp0-worktree-setup sp1-01-domain-new-types sp1-02-status-variants-state-machine \
          sp2-01-policy-diff-drift sp3-commands sp4-sp9-frontend; do
  cp "docs/superpowers/plans/contract-enhancement/${f}.md" \
     "$WT/docs/superpowers/plans/contract-enhancement/"
done
```

- [ ] **Step 4: Commit spec + plans inside the worktree**

```bash
cd .worktrees/contract-enhancement
git add docs/superpowers/
git commit -m "docs(contracts): add contract lock enhancement spec + implementation plans"
```

---

## Task 3: Run baseline tests inside the worktree

All steps below run from **inside `.worktrees/contract-enhancement/`**.

- [ ] **Step 1: Enter the worktree**

```bash
cd "$(git rev-parse --show-toplevel)/.worktrees/contract-enhancement"
```

All remaining commands in this task (and in SP1–SP9) are run from this directory.

- [ ] **Step 2: Rust baseline**

```bash
cargo test --workspace 2>&1 | tail -15
```

Expected: `test result: ok. N passed; 0 failed`.

If any tests fail → **stop**. Do not proceed to SP1. Fix or report the failures first.

- [ ] **Step 3: TypeScript baseline**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | tail -10 && cd ..
```

Expected: no output (zero errors).

If TypeScript errors are present → **stop**. Report them before proceeding.

- [ ] **Step 4: Report worktree is ready**

```
✓ Worktree ready at .worktrees/contract-enhancement
✓ Branch: feat/contract-lock-enhancement
✓ Rust tests: N passed, 0 failed
✓ TypeScript: 0 errors
→ Proceed to SP1-01
```

---

## Working Directory Reminder

**Every command in SP1-01 through SP9-01 must be run from inside the worktree:**

```bash
cd "$(git rev-parse --show-toplevel)/.worktrees/contract-enhancement"
```

The working tree at the repo root is not modified until a PR is raised and merged.
