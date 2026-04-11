# Commit and git rules

## Hard rules

- Never commit unless the user explicitly asks.
- Never amend an existing commit unless the user explicitly asks. Amending loses work when pre-commit hooks fail.
- Never use `--no-verify`, `--no-gpg-sign`, or other hook-bypassing flags unless the user explicitly asks.
- Never run destructive git commands without explicit user consent:
  - `git reset --hard`
  - `git push --force` (especially to `main`/`master` — warn loudly)
  - `git clean -fd`
  - `git branch -D`
  - `git checkout -- .` / `git restore .`
- Never update `git config`.

## When the user asks for a commit

1. Run `git status`, `git diff`, and `git log -5` in parallel to understand the changes and the repo's commit style.
2. Stage specific files by name. Do not use `git add -A` or `git add .` — they can sweep in `.env`, credentials, or other untracked files you shouldn't commit.
3. Draft a conventional commit message following the repo's existing style. Use the `/1-git-commit` skill for automation.
4. Run the commit with the message passed via `HEREDOC` to preserve formatting.
5. Run `git status` after the commit to verify success.

## When a pre-commit hook fails

- Fix the underlying issue. Do not bypass the hook.
- Re-stage the fixed files and create a **new** commit. Never `--amend` after a hook failure — the original commit did not exist, so `--amend` would modify the previous commit and potentially destroy work.

## Branch and PR hygiene

- Feature work goes on a branch, not `main`.
- Never force-push to `main` or `master`.
- PR titles: under 70 characters, conventional style (`feat(contract): ...`, `fix(sidebar): ...`).
- PR bodies use the `## Summary` + `## Test plan` template.

## Reference

- `/1-git-commit` skill — generates conventional commit messages.
