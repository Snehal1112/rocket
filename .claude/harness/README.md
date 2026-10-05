# Harness

A removable set of guard hooks, a resume skill and orchestration rules.

## Pieces

- `hooks/bash-guard`: denies `cargo test|check|clippy|build` without `-j`, `cargo test --workspace|--all`, bare `git stash` / `git stash pop`, and `git add -A|--all|.`. Matches command tokens per shell segment, so quoted text and heredoc bodies do not trigger it.
- `hooks/commit-gate`: before `git commit`, runs `yarn tsc --noEmit` and `yarn check` for staged `.ts/.tsx`, and `cargo check -j4` for staged `.rs`. Skip with `HARNESS_GATE=0`. Budget is 600 s. Paths given after `--` or in a `git add` in the same command also count.
- `../skills/resume-plan/SKILL.md`: checklist for finding where multi-plan work stands after `/clear` or compaction.
- `../rules/harness.md`: what the hooks enforce and the orchestration convention.
- `install.sh` / `uninstall.sh`: wire the hooks into `.claude/settings.json`. Both accept `--root <dir>` or `HARNESS_ROOT`.

## Install

    bash .claude/harness/install.sh

It backs up `settings.json` once to `backup/settings.json.pre-harness`, adds two `Bash` PreToolUse hooks and three `permissions.deny` entries, adds a link in `rules/00-shortcuts.md`, and writes `manifest.json`. Re-running changes nothing. Existing settings are never altered.

The hook commands look like `h="$(git rev-parse --show-toplevel 2>/dev/null)/.claude/harness/hooks/<name>"; if [ -f "$h" ]; then bash "$h"; fi`. They resolve the hook in the checkout the command runs in, not via `$CLAUDE_PROJECT_DIR` (which points at the main checkout inside a worktree). They exit 0 silently when the file is absent, so the same settings work in the main checkout before merge, in any worktree and after merge. Re-running install migrates the old `$CLAUDE_PROJECT_DIR` form in place, and uninstall removes both forms.

## Remove

- `bash .claude/harness/uninstall.sh`: removes the entries it added, leaves the files inert.
- `bash .claude/harness/uninstall.sh --purge`: also deletes `.claude/harness/`, the skill and `rules/harness.md`.
- Or `git revert` the harness commit (run uninstall first if it was installed).

After uninstall, `settings.json` is byte-identical to its pre-install state if nothing else changed meanwhile.

## Does it help

Every deny is logged to `log/guard.log` as `timestamp<TAB>rule-id<TAB>first 80 chars`. Count by rule:

    cut -f2 .claude/harness/log/guard.log | sort | uniq -c

Rules that never fire are noise and can go. Rules that fire often show a habit worth fixing at the source. Run the tests with `bash .claude/harness/tests/run.sh`.
