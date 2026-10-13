# Software Factory v2 Plan 07: Token Efficiency

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans, executed by a `factory-builder` subagent. Skip that skill's final whole-branch review and its ledger scripts, because the factory's reviewers and `ctl` do those. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Plans 01 to 06 of this series must be finished and merged first.** This plan edits files those plans already changed (`factory-ctl`, `tests/run.sh`, `.claude/commands/factory.md`, the agent prompts, `hooks/log-event`, the factory-server crate and `.claude/software-factory.md`). Later plans moved wording and line numbers, so every edit below is found by its anchor text. Re-read each file before you edit it, and apply an edit by its meaning if an earlier plan reworded the anchor. Plan 01 added `ctl show` and the stage guard, plan 02 added `record-checks`, the `prompt_has` test helper, the `UNCOMMITTED:` line and the reviewer finding tags, plan 03 added the board lock and the Sync flow, plan 05 added `stage_history` to the board and the attribution fixes to the server.

**Goal:** Cut the token cost of a factory job by keeping diffs out of the orchestrator context, trimming commit and prompt overhead, and by logging and budgeting weighted tokens per job.

**Architecture:** The orchestrator writes diffs to files by shell redirect and `ctl classify-diff` and `ctl diff-size` decide from small side files (`.names`, `.stat`). Reviewers and the approver page the diff with `offset` and `limit`. The builder commits with the skill but runs the git commands inline. The `log-event` hook adds a `tokens` object to every `SubagentStop` event by streaming the agent transcript. `ctl tokens` and `ctl budget-check` read the event log and `.claude/factory/config.json`. The dashboard shows the per-job total.

**Tech Stack:** Python 3 stdlib (`factory-ctl`, `log-event`), bash plus `jq` tests, Claude Code agent and command markdown, Rust (`tools/factory-server`), plain JavaScript page code with `node --test`.

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md`, section F (gaps F1 to F9), decisions 7 and 11, and the "Interfaces between plans" row for plan 07. Design doc: `.claude/software-factory.md`.

## Context: what job 001 measured

- About 12.75M tokens in total, 11.76M of them cache reads. With the weighted formula below, job 001 is about 2.17M weighted tokens.
- Weighted tokens (used everywhere in this plan): `input + cache_creation + output + cache_read / 10`.
- The orchestrator session was 53.6 percent of the job.
- The commit skill spawns 2 or more helper agents per commit (about 80k tokens per helper). The merge commit alone used 9 helpers.
- The builder slept 60 s and 45 s for helpers that were done in 10 s. Each sleep is a turn that re-reads the whole context.
- The same diff is read 5 times in a clean round (4 reviewers and the approver). `Read` stops at 2000 lines, so a long diff can be half-read without a warning. The diff of job 001 was only 23 KB.
- Defaults chosen from this: job budget 3,000,000 weighted tokens, warning at 80 percent, stop at 100 percent, diff cap 4000 changed lines. Both live in the new file `.claude/factory/config.json`, read by `ctl`.
- Attribution of usage to a job is approximate. The hook sees only subagent transcripts, so the orchestrator's own tokens are not in the total. The doc says so.

## Global Constraints

- One plan at a time, one builder at a time. No parallel builders in one worktree.
- Ledger writes go through `ctl` only (decision 8). Builders and reviewers never write the ledger.
- Docs stay under `.claude/`. `.claude/software-factory.md` is updated by this plan, `CLAUDE.md` stays small.
- Plan 07 produces exactly these interface names: `ctl classify-diff --names-file <path>` (prints `doc-only` or `code`), `ctl diff-size --stat-file <path>`, `ctl tokens <id>`, `ctl budget-check <id>` (prints `ok`, `warn` or `stop`), the file `.claude/factory/config.json` (keys `job_budget_weighted`, `budget_warn_percent`, `diff_cap_changed_lines`) and the `tokens` object on `SubagentStop` events (`input`, `cache_read`, `cache_creation`, `output`, `weighted`).
- All `cargo` commands pass `-j4`. Never `cargo test --workspace` or `--all`. Server tasks use `--manifest-path tools/factory-server/Cargo.toml`, and also run `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`. Rust production code has no `unwrap`.
- Commits use a pathspec: `git add <paths> && git commit --only -m "..." -- <paths>`. Invoke the skill `dev-workflow-skills:1-git-commit` for the message, but run its git commands inline with no helper agents and no amend step (decision 11). Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never push.
- Code comments are short full sentences that end with a period. No emojis.
- The command lint in `tests/run.sh` checks that every `ctl <word>` in `.claude/commands/factory.md` is a real subcommand. Do not write ordinary prose such as "through ctl only" in that file. Write `ctl` followed by a lowercase word only when the word is a subcommand.
- Follow the existing style: `factory-ctl` has one `cmd_<name>(args)` per subcommand and `die()` for errors. Tests use `check "<description>" <command...>`, `pass`, `fail` and `prompt_has` in `.claude/factory/tests/run.sh`.

## Review Focus

- A diff that mixes a doc with a hook, `Cargo.toml`, `package.json`, `tauri.conf.json` or a settings file must classify as `code`, and an empty names file must classify as `code`, so the security reviewer is never skipped by mistake. Tested in Task 1.
- A security reviewer whose last verdict was CHANGES must not be skipped because the fix diff is doc-only, because skipping clears the CHANGES. Pinned in the command text and its lint in Task 1.
- Streaming transcripts repeat a message id with growing usage, and a resumed builder stops more than once with a cumulative total. Neither may be counted twice. Tested in Task 3.
- A missing, garbage, empty or oversized transcript, a missing event log, and a missing or invalid `config.json` (zero, negative, text) must never fail the hook or `ctl`: the hook exits 0 and writes the event without `tokens`, and `ctl` uses the defaults. Tested in Task 3 (and Task 1 for the diff cap).
- A job stopped for "token budget" stays stopped after `rework` until the user raises the budget in `config.json`. The command text says so, so the user is not left in a loop. Pinned in Task 3.

---

### Task 1: Diff and context hygiene

**Files:**
- Create: `.claude/factory/config.json`
- Modify: `.claude/factory/factory-ctl` (config loader, `classify-diff`, `diff-size`)
- Modify: `.claude/factory/tests/run.sh`
- Modify: `.claude/commands/factory.md` (diff files by redirect, review and approve stages, hygiene section)
- Modify: `.claude/agents/factory-security-reviewer.md`, `factory-ux-reviewer.md`, `factory-ui-reviewer.md`, `factory-code-reviewer.md`, `factory-approver.md`

**Interfaces:**
- Consumes: from plan 01, `ctl diff-range <id>` and the review stage text. From plan 02, the `prompt_has` helper, the reviewer finding-tag paragraph that starts `Start every finding with its severity tag`, and the approver prompt.
- Produces: `load_config() -> dict` with the three keys (defaults 3000000, 80, 4000), `is_doc_path(path) -> bool`, `cmd_classify_diff`, `cmd_diff_size`. CLI `ctl classify-diff --names-file <path>` prints `doc-only` or `code`. CLI `ctl diff-size --stat-file <path>` prints `ok <n>` (exit 0) or `too-large <n> <cap>` (exit 1), where `<n>` is the insertions plus deletions from the last line of a `git diff --stat` file. The environment variable `FACTORY_CONFIG` overrides the config path (tests only).

- [ ] **Step 1: Write the failing ctl tests**

In `.claude/factory/tests/run.sh`, insert this block directly before the line `# ---- /factory command ----`:

```bash
# ---- diff hygiene: classify-diff, diff-size and config ----
names="$tmp/names.txt"
classify() { python3 "$ctl" classify-diff --names-file "$names"; }
printf '%s\n' 'docs/superpowers/plans/x.md' '.claude/commands/factory.md' 'README.md' '.claude/software-factory.md' >"$names"
check "ctl: classify-diff says doc-only for docs, .claude and markdown" test "$(classify)" = "doc-only"
printf '%s\n' 'docs/a.md' 'crates/rocket-http/src/lib.rs' >"$names"
check "ctl: classify-diff says code when one path is source" test "$(classify)" = "code"
printf '%s\n' 'docs/a.md' 'Cargo.toml' >"$names"
check "ctl: classify-diff says code for Cargo.toml" test "$(classify)" = "code"
printf '%s\n' '.claude/notes.md' 'package.json' >"$names"
check "ctl: classify-diff says code for package.json" test "$(classify)" = "code"
printf '%s\n' '.claude/notes.md' 'src-tauri/tauri.conf.json' >"$names"
check "ctl: classify-diff says code for tauri.conf.json" test "$(classify)" = "code"
printf '%s\n' '.claude/notes.md' '.claude/factory/hooks/log-event' >"$names"
check "ctl: classify-diff says code for a hooks path" test "$(classify)" = "code"
printf '%s\n' '.claude/harness/hooks/bash-guard' >"$names"
check "ctl: classify-diff says code for a harness hook" test "$(classify)" = "code"
printf '%s\n' '.claude/notes.md' '.claude/settings.json' >"$names"
check "ctl: classify-diff says code for a settings file" test "$(classify)" = "code"
: >"$names"
check "ctl: classify-diff says code for an empty names file" test "$(classify)" = "code"
check "ctl: classify-diff rejects a missing names file" \
  bash -c "! python3 '$ctl' classify-diff --names-file '$tmp/nope.txt'"

cfg="$tmp/config.json"
stat="$tmp/diff.stat"
size() { FACTORY_CONFIG="$cfg" python3 "$ctl" diff-size --stat-file "$stat"; }
printf '%s\n' '{"diff_cap_changed_lines": 10}' >"$cfg"
printf '%s\n' ' a.rs | 8 ++++++++' ' b.rs | 3 +++' ' 2 files changed, 6 insertions(+), 4 deletions(-)' >"$stat"
check "ctl: diff-size says ok at exactly the cap" test "$(size)" = "ok 10"
printf '%s\n' ' a.rs | 8 ++++++++' ' 1 file changed, 6 insertions(+), 5 deletions(-)' >"$stat"
out="$(size)"; rc=$?
check "ctl: diff-size says too-large over the cap with exit 1" test "$out" = "too-large 11 10" -a "$rc" -eq 1
printf '%s\n' ' a.rs | 3 ---' ' 1 file changed, 3 deletions(-)' >"$stat"
check "ctl: diff-size counts a deletions-only diff" test "$(size)" = "ok 3"
: >"$stat"
check "ctl: diff-size treats an empty stat file as zero lines" test "$(size)" = "ok 0"
check "ctl: diff-size rejects a missing stat file" \
  bash -c "! FACTORY_CONFIG='$cfg' python3 '$ctl' diff-size --stat-file '$tmp/nope.stat'"
printf '%s\n' '{"diff_cap_changed_lines": -5}' >"$cfg"
printf '%s\n' ' 1 file changed, 3000 insertions(+)' >"$stat"
check "ctl: a negative diff cap falls back to the default 4000" test "$(size)" = "ok 3000"
printf 'not json' >"$cfg"
printf '%s\n' ' 1 file changed, 4001 insertions(+)' >"$stat"
out="$(size)"; rc=$?
check "ctl: an invalid config file falls back to the default cap" test "$out" = "too-large 4001 4000" -a "$rc" -eq 1
check "config: the shipped diff cap is a positive integer" \
  jq -e '.diff_cap_changed_lines | (type == "number" and . > 0)' "$factory/config.json"
```

Then, in the prompt-rules block (the lines that call `prompt_has`, which plan 02 put before `# ---- factory-ctl ----`), insert this block directly before the line `# ---- factory-ctl ----`:

```bash
# Usage: prompt_lacks <agent> <text>. Pins that a rule is gone from an agent prompt.
prompt_lacks() {
  if grep -qF -- "$2" "$agents/$1.md"; then fail "prompt $1: no longer says: $2"; else pass "prompt $1: no longer says: $2"; fi
}

for role in security ux ui code; do
  prompt_has "factory-$role-reviewer" 'offset and limit 1500'
  prompt_has "factory-$role-reviewer" 'at most 60 lines'
done
prompt_has factory-approver '.stat'
prompt_has factory-approver 'offset and limit 1500'
prompt_has factory-approver 'only for the paths'
prompt_has factory-approver 'at most 60 lines'
```

Then, in the `# ---- /factory command ----` section, insert this block directly before the line `# ---- summary ----`:

```bash
for word in 'classify-diff' 'diff-size' 'doc-only' 'diff too large, split the plan' 'never by capturing output' \
  'review-<from7>..<to7>.stat' 'security=skipped' 'offset and limit 1500' 'at most 60 lines' \
  'never skipped over an open CHANGES'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL' | head -60`
Expected: `FAIL:` lines for every new `ctl: classify-diff`, `ctl: diff-size`, `config:`, `prompt` (reviewers and approver) and `command mentions` check, and a final `N failed`.

- [ ] **Step 3: Create the config file**

Create `.claude/factory/config.json` with exactly:

```json
{
  "diff_cap_changed_lines": 4000
}
```

- [ ] **Step 4: Implement the ctl changes**

Edit `.claude/factory/factory-ctl`.

4a. Directly after the line `REVIEW_STATES = (...)` (and after any constants that earlier plans put there) add:

```python
CONFIG_DEFAULTS = {
    "job_budget_weighted": 3000000,
    "budget_warn_percent": 80,
    "diff_cap_changed_lines": 4000,
}
# A path with one of these file names is never treated as a doc, wherever it lives.
CODE_FILE_NAMES = (
    "Cargo.toml",
    "package.json",
    "tauri.conf.json",
    "settings.json",
    "settings.local.json",
)
DOC_DIRS = ("docs", ".claude")
```

4b. Directly before `def build_parser():` add:

```python
def config_path():
    override = os.environ.get("FACTORY_CONFIG")
    if override:
        return override
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json")


def load_config():
    """Return the factory settings. A missing file, bad JSON or a bad value gives the default."""
    config = dict(CONFIG_DEFAULTS)
    try:
        with open(config_path(), encoding="utf-8") as handle:
            data = json.load(handle)
    except (OSError, ValueError):
        return config
    if isinstance(data, dict):
        for key, default in CONFIG_DEFAULTS.items():
            value = data.get(key, default)
            if isinstance(value, int) and not isinstance(value, bool) and value > 0:
                config[key] = value
    return config


def is_doc_path(path):
    """A doc path is markdown, or lives under docs/ or .claude/, and is not a code-like file."""
    parts = path.split("/")
    if "hooks" in parts[:-1] or parts[-1] in CODE_FILE_NAMES:
        return False
    return path.endswith(".md") or parts[0] in DOC_DIRS


def read_text(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except OSError as err:
        die(f"cannot read {path}: {err.strerror}")


def cmd_classify_diff(args):
    paths = [line.strip() for line in read_text(args.names_file).splitlines() if line.strip()]
    # An empty list is never doc-only, so an empty diff cannot skip the security review.
    print("doc-only" if paths and all(is_doc_path(path) for path in paths) else "code")


def cmd_diff_size(args):
    lines = [line for line in read_text(args.stat_file).splitlines() if line.strip()]
    changed = 0
    if lines:
        changed = sum(int(n) for n in re.findall(r"(\d+) (?:insertion|deletion)", lines[-1]))
    cap = load_config()["diff_cap_changed_lines"]
    if changed > cap:
        print(f"too-large {changed} {cap}")
        sys.exit(1)
    print(f"ok {changed}")
```

4c. In `build_parser`, directly before `return parser` add:

```python
    classify = sub.add_parser("classify-diff")
    classify.add_argument("--names-file", dest="names_file", required=True)
    classify.set_defaults(func=cmd_classify_diff)

    diff_size = sub.add_parser("diff-size")
    diff_size.add_argument("--stat-file", dest="stat_file", required=True)
    diff_size.set_defaults(func=cmd_diff_size)
```

- [ ] **Step 5: Add the paging and reply-length paragraph to the four reviewers**

In each of `.claude/agents/factory-security-reviewer.md`, `factory-ux-reviewer.md`, `factory-ui-reviewer.md` and `factory-code-reviewer.md`, insert the paragraph below, followed by a blank line, directly before the paragraph that starts `Start every finding with its severity tag` (plan 02 added it).

```
Reading the diff: `Read` returns at most 2000 lines per call, so a long diff can be half-read without a warning. Read the diff file in pages with offset and limit 1500 (offset 1, then 1501, then 3001, and so on) until a call returns fewer than 1500 lines. Never judge a diff you only partly read. Keep your reply to at most 60 lines: list findings only, with no summary of the diff and no praise.
```

- [ ] **Step 6: Update the approver prompt**

In `.claude/agents/factory-approver.md`, change the `Input:` paragraph so that it also lists the diff stat file: after the words `the full diff file path,` insert `the diff stat file path (\`review-*.stat\`, one line per changed path with its line count),`. Then insert this paragraph, followed by a blank line, directly before the line that starts `Decide ESCALATE if any of these is true`:

```
Reading the diff: start with the `.stat` file and the review reports. Open the diff file only for the paths that a risk rule below flags: `Cargo.toml`, `package.json`, `tauri.conf.json`, `.yml` files, hook files, `src-tauri/`, and any path whose name suggests the trust gate, auth, secrets, `env`, events, DTOs, models or schemas. When you are unsure whether a path is flagged, read it. Read hunks only for the paths that matter, and use offset and limit 1500 per `Read` call, because `Read` stops at 2000 lines. Docs, plans and test-only paths never need hunks. Keep your reply to at most 60 lines.
```

- [ ] **Step 7: Edit the command text**

Edit `.claude/commands/factory.md`. Earlier plans reworded these places, so find each by the quoted start and keep their other additions.

(a) Add this section directly above `## status`:

```markdown
## Diff hygiene

Every diff is written to a file by a shell redirect (`git -C "<main>" diff <range> > <file>`), never by capturing output, and the orchestrator never Reads, prints or pastes a diff. Reviewers and the approver read the files. This applies to the review, approve and Sync flows alike: a diff that `ctl` prints (for example `ctl sync-diff`) is also saved by redirect. Only the small `.names` and `.stat` side files are used to decide.
```

(b) In stage `review`, step 1 (the step that starts `Run \`ctl diff-range <id>\``). Replace the sentence that starts ``Write `git -C "<main>" diff <from>..<to>` to the job directory as`` (up to the end of that step) with:

```
Write three files in the job directory by redirect, never by capturing output: `git -C "<main>" diff <from>..<to> > review-<from7>..<to7>.diff`, `git -C "<main>" diff --stat <from>..<to> > review-<from7>..<to7>.stat` and `git -C "<main>" diff --name-only <from>..<to> > review-<from7>..<to7>.names`, where `<from7>` and `<to7>` are the first 7 characters of each sha. Never Read or print the diff. Run `ctl diff-size --stat-file <job dir>/review-<from7>..<to7>.stat`. If it exits 1 (`too-large`), run `ctl set-stage <id> needs-human --reason "diff too large, split the plan"` and stop. Then run `ctl classify-diff --names-file <job dir>/review-<from7>..<to7>.names`. If it prints `doc-only`, the security reviewer is skipped: `ctl set-stage <id> review --review security=skipped`. A skipped reviewer counts as pass. The security reviewer is never skipped over an open CHANGES: if the newest `review-security-r<n>.md` (by modification time) ends with `VERDICT: CHANGES`, it runs even for a doc-only diff, because skipping would clear the CHANGES without a re-check.
```

(c) In stage `review`, the step that dispatches the reviewers (it ends `Reviewers never run cargo or yarn.`). Append:

```
Tell each reviewer to read the diff in pages with offset and limit 1500 and to keep its reply at most 60 lines. Do not dispatch the security reviewer when step 1 recorded it as skipped.
```

(d) In stage `approve`, the sentence that starts `Only then write the full diff` (plan 01) writes the full diff. Replace the part from `write the full diff` up to the next sentence that starts `Dispatch` with:

```
write the full job diff by redirect, never by capturing output: `git -C "<main>" diff <base>..<final head> > review-<base7>..<head7>.diff`, the same range with `--stat` to `review-<base7>..<head7>.stat` and with `--name-only` to `review-<base7>..<head7>.names`. Never Read or print the diff. Run `ctl diff-size --stat-file <job dir>/review-<base7>..<head7>.stat`. If it exits 1, run `ctl set-stage <id> needs-human --reason "diff too large, split the plan"` and stop.
```

Then find the sentence that starts ``Dispatch `factory-approver` `` and add this sentence directly after it:

```
Also give it the path of the `.stat` file and tell it: read the stat file and the review reports first, open the diff only for the paths its risk rules flag, with offset and limit 1500, and keep the reply at most 60 lines.
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed` and no `FAIL:` line. If `command contract: ctl <word> is a real subcommand` fails, a sentence in `factory.md` has `ctl` followed by a lowercase word that is not a subcommand: reword it.

- [ ] **Step 9: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, running its git commands inline with no helper agents and no amend step. Suggested message: `feat(factory): keep diffs out of the orchestrator and page them in reviewers`. Then:

```bash
git add .claude/factory/config.json .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md .claude/agents/factory-security-reviewer.md .claude/agents/factory-ux-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/agents/factory-code-reviewer.md .claude/agents/factory-approver.md && git commit --only -m "<message from the skill>" -- .claude/factory/config.json .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md .claude/agents/factory-security-reviewer.md .claude/agents/factory-ux-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/agents/factory-code-reviewer.md .claude/agents/factory-approver.md
```

---

### Task 2: Commit and helper ceremony, prompt trims, orchestrator hygiene

**Files:**
- Modify: `.claude/agents/factory-builder.md`
- Modify: `.claude/agents/factory-spec-writer.md`, `factory-code-reviewer.md`, `factory-ui-reviewer.md`
- Modify: `.claude/commands/factory.md` (context hygiene section, stall note)
- Modify: `.claude/factory/tests/run.sh`

**Interfaces:**
- Consumes: `prompt_has` and `prompt_lacks` from Task 1 and plan 02. The builder prompt from plans 02 and 04 (bullets under "Rocket hard rules").
- Produces: the builder rules (inline commit, helper cap, no `cd`, no heredoc), the trimmed read lists, and a `## Context hygiene` section in the command.

- [ ] **Step 1: Write the failing lint tests**

In `.claude/factory/tests/run.sh`, insert this block directly before the line `# ---- factory-ctl ----` (after the block from Task 1):

```bash
prompt_has factory-builder 'git commands inline'
prompt_has factory-builder 'Do not spawn helper agents for a commit'
prompt_has factory-builder 'amend'
prompt_has factory-builder 'at most 2 helper agents per task'
prompt_has factory-builder 'Never use `cd`'
prompt_has factory-builder 'heredoc'
prompt_has factory-builder 'Edit and Write tools'
prompt_has factory-builder 'conventional message'
prompt_lacks factory-spec-writer 'Read `CLAUDE.md`'
prompt_has factory-spec-writer 'already in your context'
prompt_has factory-spec-writer 'opencollection-spec-reference.md'
prompt_lacks factory-code-reviewer 'Read `.claude/rules/rust-ddd-boundaries.md`'
prompt_has factory-code-reviewer 'already in your context'
prompt_has factory-code-reviewer 'crate `CLAUDE.md`'
prompt_lacks factory-ui-reviewer 'frontend-component-guardrails.md` first'
prompt_has factory-ui-reviewer 'already in your context'
```

In the `# ---- /factory command ----` section, insert this block directly before the line `# ---- summary ----`:

```bash
for word in '## Context hygiene' 'fresh or lean session' 'last lines of an agent reply' 'Never paste a diff' \
  'no events for 10 minutes' 'Edit and Write tools'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL: (prompt factory-(builder|spec-writer|code-reviewer|ui-reviewer)|command mentions)'`
Expected: `FAIL:` for the new builder, spec-writer, code-reviewer and ui-reviewer lines and for the six new command words. The `prompt_lacks` lines FAIL too, because the old text is still there.

- [ ] **Step 3: Edit the builder prompt**

In `.claude/agents/factory-builder.md`, under the "Rocket hard rules" list:

(a) Replace the whole bullet that starts ``- Commit each task with the `dev-workflow-skills:1-git-commit` skill.`` with:

```
- Commit each task once, with the `dev-workflow-skills:1-git-commit` skill, but run the skill's git commands inline yourself (`git diff --cached`, `git log -5`, the commit). Do not spawn helper agents for a commit, not even if the skill tells you to: each helper costs about 80,000 tokens. Skip the skill's amend step too. One commit per task is one logical change, and earlier commits are never rewritten. If the skill still insists on helpers, do not use it for that commit: write a conventional message such as `feat(<area>): <task title>` and say in your reply that you did. Use a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never run a bare `git stash`.
```

(b) Directly after the whole bullet that starts ``- Never write to `progress.md` `` add these two bullets:

```
- Spawn at most 2 helper agents per task, and none for a commit. Do the work yourself unless a helper is clearly cheaper.
- Never use `cd` to leave your worktree, and never append to a file with a heredoc (`cat >> file <<EOF`) or `echo >> file` in Bash. The sandbox refuses both, and each refusal costs a full turn. Use `git -C <path>` and absolute paths. Use the Edit and Write tools to change files.
```

- [ ] **Step 4: Trim the read lists**

(a) `.claude/agents/factory-spec-writer.md`, step 1. Replace the sentence ``Read `CLAUDE.md`, `.claude/rules/00-shortcuts.md` and the rule file for each area the feature touches.`` with:

```
The root `CLAUDE.md` and the files in `.claude/rules/` are already in your context, so do not read them again. Read the `CLAUDE.md` of each crate the feature touches (`crates/<name>/CLAUDE.md`, these are not preloaded).
```

Keep the rest of that step (the sentence about `docs/superpowers/specs/opencollection-spec-reference.md`).

(b) `.claude/agents/factory-code-reviewer.md`, the `Input:` paragraph. Replace the sentence ``Read `.claude/rules/rust-ddd-boundaries.md`, `.claude/rules/tauri-ipc-boundaries.md` and the crate `CLAUDE.md` files for the crates the diff touches.`` with:

```
The root `CLAUDE.md` and the files in `.claude/rules/` are already in your context, so do not read them. Read the crate `CLAUDE.md` files for the crates the diff touches (these are not preloaded).
```

(c) `.claude/agents/factory-ui-reviewer.md`, the `Input:` paragraph. Replace the sentence ``Read `.claude/rules/frontend-component-guardrails.md` first.`` with:

```
The rules in `.claude/rules/` are already in your context, so do not read them.
```

- [ ] **Step 5: Add the context hygiene section and the stall note**

In `.claude/commands/factory.md`, add this section directly above `## status` (below the `## Diff hygiene` section from Task 1):

```markdown
## Context hygiene

The orchestrator session re-reads its whole context every turn, so keep it small.

- Run `/factory` in a fresh or lean session. An old, long session carries all of its baggage into every dispatch decision.
- Act on the last lines of an agent reply: the `VERDICT:` line, the `DECISION:` line, the `CHECKS:` block and the four handoff lines. Do not repeat a reply in your own text.
- Never paste a diff, never Read a diff file, never print a log. See "Diff hygiene".
- Use the Edit and Write tools to change files. Never use a heredoc append (`cat >> file <<EOF`) or `cd` out of the working directory in Bash: the sandbox refuses both and each refusal costs a turn.
- While a builder runs in the background, do not poll in a loop. When you do check, read only its newest event time: `jq -r 'select(.agent_id=="<agent id>") | .ts' "<main>/.superpowers/factory/events.jsonl" | tail -n 1`. If there have been no events for 10 minutes (compare with `date +%s`), tell the user and ask whether to wait, stop it or resume it. Do not decide alone.
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed` and no `FAIL:` line.

- [ ] **Step 7: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, running its git commands inline with no helper agents and no amend step. Suggested message: `feat(factory): cap commit ceremony and helpers, trim preloaded reads`. Then:

```bash
git add .claude/agents/factory-builder.md .claude/agents/factory-spec-writer.md .claude/agents/factory-code-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/commands/factory.md .claude/factory/tests/run.sh && git commit --only -m "<message from the skill>" -- .claude/agents/factory-builder.md .claude/agents/factory-spec-writer.md .claude/agents/factory-code-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/commands/factory.md .claude/factory/tests/run.sh
```

---

### Task 3: Token logging and budget, dashboard display, doc sync, final verification

**Files:**
- Modify: `.claude/factory/hooks/log-event`
- Modify: `.claude/factory/factory-ctl` (`tokens`, `budget-check`, status column)
- Modify: `.claude/factory/config.json` (budget keys)
- Modify: `.claude/factory/tests/run.sh`
- Modify: `.claude/commands/factory.md` (budget check before each dispatch)
- Modify: `tools/factory-server/src/model.rs`, `events.rs`, `state.rs`
- Modify: `tools/factory-server/static/board.js`, `dashboard.css`
- Modify: `tools/factory-server/tests/board.test.js`
- Modify: `.claude/software-factory.md`

**Interfaces:**
- Consumes: from Task 1, `load_config()` and `CONFIG_DEFAULTS` in `factory-ctl`. From plan 05, the board field `stage_history` (list of `{"stage", "ts"}`), the hook's `KEEP` list and `build_event`, the server attribution in `build_state`. From plan 03, `ctl` mutating calls run under the board lock. The two new subcommands only read, so they stay out of the lock set.
- Produces: the `tokens` object `{"input", "cache_read", "cache_creation", "output", "weighted"}` on `SubagentStop` events with a readable transcript. `ctl tokens <id>` prints an integer. `ctl budget-check <id>` prints `ok`, `warn` or `stop` (details on stderr). The server fields `AgentState.weighted_tokens: u64` and `JobView.tokens: u64`. JS `FactoryBoard.formatTokens(n)` and `FactoryBoard.tokensLine(job)`.

#### Part A: the hook

- [ ] **Step 1: Write the failing hook tests**

In `.claude/factory/tests/run.sh`, insert this block directly after the check `logger: a worktree cwd logs to the main checkout` (before the `# ---- settings ----` line):

```bash
# ---- log-event: token usage at SubagentStop ----
tlog="$tmp/tokens-events.jsonl"
transcript="$tmp/transcript.jsonl"
{
  printf '%s\n' '{"type":"assistant","message":{"id":"m1","usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":5}}}'
  printf '%s\n' '{"type":"assistant","message":{"id":"m1","usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":50}}}'
  printf '%s\n' '{"type":"assistant","message":{"id":"m2","usage":{"input_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":2000,"output_tokens":4}}}'
  printf '%s\n' 'not json'
  printf '%s\n' '{"type":"user","message":{"content":"hi"}}'
} >"$transcript"
stop_event() {
  printf '{"hook_event_name":"SubagentStop","agent_id":"t1","agent_type":"factory-builder","agent_transcript_path":"%s","cwd":"/x"}' "$1" |
    FACTORY_EVENT_LOG="$tlog" python3 "$logger"
}
stop_event "$transcript"
check "logger: SubagentStop gets summed tokens, one usage per message id" \
  jq -e 'select(.tokens.input==11 and .tokens.cache_creation==100 and .tokens.cache_read==3000 and .tokens.output==54)' "$tlog"
check "logger: weighted is input + creation + output + cache_read / 10" \
  jq -e 'select(.tokens.weighted==465)' "$tlog"
stop_event "$tmp/no-such-transcript.jsonl"
rc=$?
check "logger: a missing transcript exits 0" test "$rc" -eq 0
check "logger: a missing transcript still logs the event without tokens" \
  jq -e -s '.[1] | select(.hook_event_name=="SubagentStop" and (has("tokens") | not))' "$tlog"
printf 'garbage only\n' >"$tmp/garbage.jsonl"
stop_event "$tmp/garbage.jsonl"
check "logger: a garbage transcript logs the event without tokens" \
  jq -e -s '.[2] | select(has("tokens") | not)' "$tlog"
head -c 21000000 /dev/zero | tr '\0' 'x' >"$tmp/huge.jsonl"
stop_event "$tmp/huge.jsonl"
rc=$?
check "logger: an oversized transcript exits 0" test "$rc" -eq 0
check "logger: an oversized transcript logs the event without tokens" \
  jq -e -s '.[3] | select(has("tokens") | not)' "$tlog"
printf '{"hook_event_name":"PostToolUse","agent_id":"t1","tool_name":"Read","agent_transcript_path":"%s","cwd":"/x"}' "$transcript" |
  FACTORY_EVENT_LOG="$tlog" python3 "$logger"
check "logger: only SubagentStop events carry tokens" \
  jq -e -s '.[4] | select(.tool_name=="Read" and (has("tokens") | not))' "$tlog"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL: logger'`
Expected: `FAIL:` for the two checks that expect `tokens`. The "still logs without tokens" checks already pass today, which is fine: they pin that the hook never breaks.

- [ ] **Step 3: Implement the hook change**

In `.claude/factory/hooks/log-event`:

3a. Directly after the line `MAX_COMMAND = 80` (or after the last constant, if plan 05 added more) add:

```python
MAX_TRANSCRIPT_BYTES = 20 * 1024 * 1024
USAGE_FIELDS = (
    ("input", "input_tokens"),
    ("cache_creation", "cache_creation_input_tokens"),
    ("cache_read", "cache_read_input_tokens"),
    ("output", "output_tokens"),
)
```

3b. Directly before `def build_event(payload):` add:

```python
def count(value):
    """Return a non-negative int, or 0 for anything else."""
    if isinstance(value, int) and not isinstance(value, bool) and value > 0:
        return value
    return 0


def transcript_tokens(path):
    """Sum the token usage of a transcript, once per unique message id.

    A streamed message repeats its id with growing usage, so the last line of an id wins.
    The read is capped. Any problem gives None, and this function never raises.
    """
    try:
        if not isinstance(path, str) or not os.path.isfile(path):
            return None
        with open(path, "rb") as handle:
            data = handle.read(MAX_TRANSCRIPT_BYTES + 1)
        if len(data) > MAX_TRANSCRIPT_BYTES:
            data = data[: data.rfind(b"\n") + 1]
        usage_by_id = {}
        for raw in data.splitlines():
            try:
                entry = json.loads(raw)
            except ValueError:
                continue
            message = entry.get("message") if isinstance(entry, dict) else None
            if not isinstance(message, dict):
                continue
            usage = message.get("usage")
            message_id = message.get("id")
            if isinstance(usage, dict) and isinstance(message_id, str):
                usage_by_id[message_id] = usage
        if not usage_by_id:
            return None
        tokens = {name: 0 for name, _ in USAGE_FIELDS}
        for usage in usage_by_id.values():
            for name, key in USAGE_FIELDS:
                tokens[name] += count(usage.get(key))
        tokens["weighted"] = (
            tokens["input"] + tokens["cache_creation"] + tokens["output"] + tokens["cache_read"] // 10
        )
        return tokens
    except Exception:
        return None
```

3c. In `build_event`, directly before its `return event` line add:

```python
    if payload.get("hook_event_name") == "SubagentStop":
        tokens = transcript_tokens(payload.get("agent_transcript_path"))
        if tokens:
            event["tokens"] = tokens
```

- [ ] **Step 4: Run the hook tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed`.

#### Part B: ctl tokens, budget-check, status and the command

- [ ] **Step 5: Write the failing ctl tests**

In `.claude/factory/tests/run.sh`, insert this block directly before the line `# ---- /factory command ----` (after the Task 1 block):

```bash
# ---- tokens and budget ----
tr="$tmp/tr"
git init -q -b main "$tr"
git -C "$tr" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
tl() { FACTORY_ROOT="$tr" FACTORY_CONFIG="$tmp/budget.json" python3 "$ctl" "$@"; }
tboard="$tr/.superpowers/factory/board.json"
tevents="$tr/.superpowers/factory/events.jsonl"
printf '%s\n' '{"job_budget_weighted": 10000, "budget_warn_percent": 80}' >"$tmp/budget.json"
tl new-job "Token job" --date 2026-10-13 >/dev/null
tl set-stage 1 build --worktree /wt/agent-x --builder-id agent-b >/dev/null
jq '.jobs[0].stage_history = [{"stage":"spec","ts":100},{"stage":"build","ts":200},{"stage":"review","ts":300}] | .jobs[0].stage = "review"' \
  "$tboard" >"$tboard.tmp" && mv "$tboard.tmp" "$tboard"
mkdir -p "$(dirname "$tevents")"
stopline() {
  # Usage: stopline <ts> <agent id> <agent type> <cwd> <weighted>
  printf '{"ts":%s,"hook_event_name":"SubagentStop","agent_id":"%s","agent_type":"%s","cwd":"%s","tokens":{"input":1,"cache_read":0,"cache_creation":0,"output":1,"weighted":%s}}\n' "$1" "$2" "$3" "$4" "$5"
}
{
  stopline 210 agent-b factory-builder /wt/agent-x/src 1000
  stopline 290 agent-b factory-builder /wt/agent-x 1500
  stopline 250 h1 general-purpose /wt/agent-x 200
  stopline 260 u1 general-purpose /elsewhere 5000
  stopline 150 s1 factory-spec-writer "$tr" 300
  stopline 350 r1 factory-code-reviewer "$tr" 400
  stopline 250 r2 factory-code-reviewer "$tr" 500
  printf '%s\n' 'not json' '{"ts":400,"hook_event_name":"SubagentStop","agent_id":"n1","agent_type":"factory-builder","cwd":"/wt/agent-x"}'
} >"$tevents"
check "ctl: tokens counts a resumed builder once, at its latest total" test "$(tl tokens 1)" = "2400"
check "ctl: tokens skips unrelated agents, other-stage agents, bad lines and events without tokens" test "$(tl tokens 1)" -eq 2400
check "ctl: tokens rejects an unknown job" bash -c "! FACTORY_ROOT='$tr' python3 '$ctl' tokens 99"
check "ctl: tokens is 0 when there is no event log" \
  bash -c "mv '$tevents' '$tevents.off'; test \"\$(FACTORY_ROOT='$tr' python3 '$ctl' tokens 1)\" = 0; mv '$tevents.off' '$tevents'"
check "ctl: status shows the tokens" bash -c "FACTORY_ROOT='$tr' FACTORY_CONFIG='$tmp/budget.json' python3 '$ctl' status | grep -q 'tokens 2k'"
check "ctl: budget-check says ok below the warning line" test "$(tl budget-check 1 2>/dev/null)" = "ok"
stopline 360 h2 general-purpose /wt/agent-x 5600 >>"$tevents"
check "ctl: budget-check says warn at 80 percent" test "$(tl budget-check 1 2>/dev/null)" = "warn"
stopline 370 h3 general-purpose /wt/agent-x 2000 >>"$tevents"
check "ctl: budget-check says stop at 100 percent" test "$(tl budget-check 1 2>/dev/null)" = "stop"
check "ctl: budget-check puts the numbers on stderr" \
  bash -c "FACTORY_ROOT='$tr' FACTORY_CONFIG='$tmp/budget.json' python3 '$ctl' budget-check 1 2>&1 >/dev/null | grep -q 'used 10000 of 10000'"
printf '%s\n' '{"job_budget_weighted": 0, "budget_warn_percent": "high"}' >"$tmp/budget.json"
check "ctl: a bad budget config falls back to the default budget" test "$(tl budget-check 1 2>/dev/null)" = "ok"
check "config: the shipped budget is 3000000 weighted with a warning at 80 percent" \
  jq -e '.job_budget_weighted == 3000000 and .budget_warn_percent == 80' "$factory/config.json"
```

In the `# ---- /factory command ----` section, insert this block directly before the line `# ---- summary ----`:

```bash
for word in 'budget-check' '`ctl tokens' 'token budget' 'config.json' 'job_budget_weighted'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL' | head -30`
Expected: `FAIL:` for the `ctl: tokens`, `ctl: status shows the tokens`, `ctl: budget-check`, `config: the shipped budget` and the five new `command mentions` checks.

- [ ] **Step 7: Implement ctl tokens, budget-check and the status column**

Edit `.claude/factory/factory-ctl`.

7a. Directly after the `DOC_DIRS = ...` constant (from Task 1) add:

```python
# Agents that run from the orchestrator checkout, not from the builder worktree.
OUTER_STAGES = ("spec", "review", "approve")
OUTER_AGENTS = (
    "factory-spec-writer",
    "factory-security-reviewer",
    "factory-ux-reviewer",
    "factory-ui-reviewer",
    "factory-code-reviewer",
    "factory-approver",
)
```

7b. Directly before `def build_parser():` add:

```python
def load_events(root):
    """Read the SubagentStop lines of the hook log. A missing file or a bad line is skipped."""
    events = []
    path = os.path.join(root, ".superpowers", "factory", "events.jsonl")
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            for line in handle:
                if '"SubagentStop"' not in line:
                    continue
                try:
                    event = json.loads(line)
                except ValueError:
                    continue
                if isinstance(event, dict):
                    events.append(event)
    except OSError:
        pass
    return events


def path_under(path, base):
    if not isinstance(path, str) or not isinstance(base, str) or not base:
        return False
    path = os.path.normpath(path)
    base = os.path.normpath(base)
    return path == base or path.startswith(base.rstrip(os.sep) + os.sep)


def is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def outer_windows(job):
    """Return the (start, end) spans in which the job sat in a stage that runs outside its worktree."""
    history = job.get("stage_history")
    if not isinstance(history, list) or not history:
        if job.get("stage") in OUTER_STAGES:
            return [(float(job.get("stage_since") or 0), float("inf"))]
        return []
    windows = []
    for index, entry in enumerate(history):
        if not isinstance(entry, dict) or entry.get("stage") not in OUTER_STAGES:
            continue
        start = entry.get("ts")
        if not is_number(start):
            continue
        end = float("inf")
        if index + 1 < len(history) and isinstance(history[index + 1], dict):
            following = history[index + 1].get("ts")
            if is_number(following):
                end = following
        windows.append((start, end))
    return windows


def job_tokens(root, job, events):
    """Sum the weighted tokens of the agents that belong to a job. The attribution is approximate.

    An event belongs to the job when its cwd is under the job worktree, when its agent is the
    recorded builder, or when a factory agent that runs from the checkout stopped while the job
    sat in the spec, review or approve stage. An agent that stopped more than once reports a
    cumulative total, so only its largest total counts.
    """
    worktree = job.get("worktree")
    builder_id = job.get("builder_id")
    windows = outer_windows(job)
    per_agent = {}
    for event in events:
        tokens = event.get("tokens")
        agent_id = event.get("agent_id")
        weighted = tokens.get("weighted") if isinstance(tokens, dict) else None
        if not agent_id or not isinstance(weighted, int) or isinstance(weighted, bool):
            continue
        mine = path_under(event.get("cwd"), worktree) or agent_id == builder_id
        stamp = event.get("ts")
        if not mine and event.get("agent_type") in OUTER_AGENTS and path_under(event.get("cwd"), root) and is_number(stamp):
            mine = any(start <= stamp < end for start, end in windows)
        if mine:
            per_agent[agent_id] = max(per_agent.get(agent_id, 0), weighted)
    return sum(per_agent.values())


def format_tokens(count):
    if count < 1000:
        return str(count)
    if count < 1000000:
        return f"{count // 1000}k"
    return f"{count / 1000000:.2f}M"


def budget_state(used, budget, warn_percent):
    if used >= budget:
        return "stop"
    if used * 100 >= budget * warn_percent:
        return "warn"
    return "ok"


def cmd_tokens(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    print(job_tokens(root, job, load_events(root)))


def cmd_budget_check(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    config = load_config()
    used = job_tokens(root, job, load_events(root))
    budget = config["job_budget_weighted"]
    print(budget_state(used, budget, config["budget_warn_percent"]))
    print(f"used {used} of {budget} weighted tokens ({used * 100 // budget} percent)", file=sys.stderr)
```

7c. In `cmd_status`, load the events once before the `for job in board["jobs"]:` loop and add the tokens to each line. Before the loop add:

```python
    root = repo_root()
    events = load_events(root)
```

(if `cmd_status` already has a `root` variable, reuse it and add only the `events` line). Directly before the line `if job.get("reason"):` inside the loop (plan 01 added it) add:

```python
        line += f"  tokens {format_tokens(job_tokens(root, job, events))}"
```

If plan 01 or 04 changed the status line so that `line` is not a variable, build the text the same way and append `  tokens <n>` before the reason suffix.

7d. In `build_parser`, directly before `return parser` add:

```python
    tokens = sub.add_parser("tokens")
    tokens.add_argument("id")
    tokens.set_defaults(func=cmd_tokens)

    budget = sub.add_parser("budget-check")
    budget.add_argument("id")
    budget.set_defaults(func=cmd_budget_check)
```

These two commands only read the board, so do not add them to the `MUTATING` set that plan 03 created.

7e. Replace `.claude/factory/config.json` with:

```json
{
  "job_budget_weighted": 3000000,
  "budget_warn_percent": 80,
  "diff_cap_changed_lines": 4000
}
```

- [ ] **Step 8: Add the budget check to the command**

In `.claude/commands/factory.md`, in the `## next` section, directly after the paragraph that starts `Take the oldest job whose stage is not`, add:

```
Before you dispatch any agent in a stage (the builder, a reviewer, the approver, a sync), run `ctl budget-check <id>`. It prints `ok`, `warn` or `stop`. On `ok`, go on. On `warn`, go on and tell the user the weighted total from `ctl tokens <id>`. On `stop`, run `ctl set-stage <id> needs-human --reason "token budget"` and stop. The budget and its warning line are `job_budget_weighted` and `budget_warn_percent` in `.claude/factory/config.json`, and the total counts weighted tokens of the job's subagents only (input plus cache creation plus output plus cache reads divided by 10). A job stopped for the token budget stays stopped after `/factory rework` until the user raises `job_budget_weighted` in that file, so say that in your report.
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed`. If `command contract: ctl <word>` fails, reword the sentence that has `ctl` followed by a word that is not a subcommand.

- [ ] **Step 10: Commit the hook, ctl and command**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, running its git commands inline with no helper agents and no amend step. Suggested message: `feat(factory): log weighted tokens per agent and enforce a job budget`. Then:

```bash
git add .claude/factory/hooks/log-event .claude/factory/factory-ctl .claude/factory/config.json .claude/factory/tests/run.sh .claude/commands/factory.md && git commit --only -m "<message from the skill>" -- .claude/factory/hooks/log-event .claude/factory/factory-ctl .claude/factory/config.json .claude/factory/tests/run.sh .claude/commands/factory.md
```

#### Part C: the dashboard

- [ ] **Step 11: Write the failing Rust tests**

In `tools/factory-server/src/events.rs`, in the existing `#[cfg(test)] mod tests` block (create `use super::*;` and the module if an earlier plan moved the tests), add:

```rust
    #[test]
    fn a_stop_event_records_the_weighted_tokens() {
        let mut registry = Registry::new();
        registry.ingest_line(
            r#"{"ts":1.0,"hook_event_name":"SubagentStart","agent_id":"a1","agent_type":"factory-builder"}"#,
        );
        registry.ingest_line(
            r#"{"ts":2.0,"hook_event_name":"SubagentStop","agent_id":"a1","agent_type":"factory-builder","tokens":{"input":10,"cache_read":1000,"cache_creation":5,"output":20,"weighted":135}}"#,
        );
        let agents = registry.agents();
        assert_eq!(agents[0].weighted_tokens, 135);
    }

    #[test]
    fn a_later_smaller_total_never_lowers_the_tokens_of_an_agent() {
        let mut registry = Registry::new();
        registry.ingest_line(
            r#"{"ts":2.0,"hook_event_name":"SubagentStop","agent_id":"a1","agent_type":"factory-builder","tokens":{"weighted":900}}"#,
        );
        registry.ingest_line(
            r#"{"ts":3.0,"hook_event_name":"SubagentStop","agent_id":"a1","agent_type":"factory-builder","tokens":{"weighted":400}}"#,
        );
        assert_eq!(registry.agents()[0].weighted_tokens, 900);
    }

    #[test]
    fn a_stop_event_with_bad_or_missing_tokens_counts_zero_and_is_not_skipped() {
        let mut registry = Registry::new();
        registry.ingest_line(
            r#"{"ts":1.0,"hook_event_name":"SubagentStart","agent_id":"a1","agent_type":"factory-builder"}"#,
        );
        let accepted = registry.ingest_line(
            r#"{"ts":2.0,"hook_event_name":"SubagentStop","agent_id":"a1","agent_type":"factory-builder","tokens":"lots"}"#,
        );
        assert!(accepted);
        assert_eq!(registry.skipped_lines, 0);
        assert_eq!(registry.agents()[0].weighted_tokens, 0);
    }
```

In `tools/factory-server/src/state.rs`, in its tests module, add (reusing the `job`, `start` helpers and `Registry` import that are already there):

```rust
    #[test]
    fn job_tokens_sum_the_weighted_tokens_of_its_agents() {
        let mut board = Board::default();
        board.jobs.push(job("001", "review", 10.0));
        let mut registry = Registry::new();
        for (i, kind) in ["factory-security-reviewer", "factory-code-reviewer"]
            .iter()
            .enumerate()
        {
            let id = format!("r{i}");
            let weighted = 1000 * (i as u64 + 1);
            start(&mut registry, 20.0, &id, kind, "/repo");
            registry.ingest_line(&format!(
                r#"{{"ts":30.0,"hook_event_name":"SubagentStop","agent_id":"{id}","agent_type":"{kind}","tokens":{{"weighted":{weighted}}}}}"#
            ));
        }
        let view = build_state(&board, &registry, 40.0, None);
        assert_eq!(view.jobs[0].tokens, 3000);
    }

    #[test]
    fn a_job_without_token_data_has_zero_tokens_in_the_json() -> TestResult {
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 100.0));
        let view = build_state(&board, &Registry::new(), 160.0, None);
        let json = serde_json::to_value(&view)?;
        assert_eq!(json["jobs"][0]["tokens"], 0);
        Ok(())
    }
```

- [ ] **Step 12: Run the Rust tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml tokens`
Expected: a compile error such as `no field weighted_tokens on type AgentState` and `no field tokens on type JobView`.

- [ ] **Step 13: Implement the Rust changes**

13a. `tools/factory-server/src/model.rs`: in `struct Event`, after the `last_message` field add:

```rust
    /// Token usage that the hook adds at `SubagentStop`. Kept as raw JSON so a bad value is not fatal.
    #[serde(default)]
    pub tokens: Option<serde_json::Value>,
```

Directly after the `Event` struct add:

```rust
impl Event {
    /// The weighted token total of a `SubagentStop` event, or 0 when it has none.
    pub fn weighted_tokens(&self) -> u64 {
        self.tokens
            .as_ref()
            .and_then(|tokens| tokens.get("weighted"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    }
}
```

In `struct AgentState`, after the `cwd` field add:

```rust
    /// Weighted tokens (input + cache creation + output + cache reads / 10) from the last stop.
    pub weighted_tokens: u64,
```

13b. `tools/factory-server/src/events.rs`: in every `AgentState { ... }` literal (the `entry` function) add `weighted_tokens: 0,` after the `cwd` line. In `on_stop`, directly after the line `agent.ended_ts = Some(event.ts);` add:

```rust
        // A resumed agent reports a cumulative total at each stop, so the largest one is the truth.
        agent.weighted_tokens = agent.weighted_tokens.max(event.weighted_tokens());
```

13c. `tools/factory-server/src/state.rs`: in `struct JobView` add a field `pub tokens: u64,` after `seconds_in_stage`. In the `JobView { ... }` literal inside `build_state` add `tokens: 0,` after `seconds_in_stage`. In the loop that sorts each view's agents (`for view in &mut jobs {` with `view.agents.sort_by(...)`), add as the last statement of the loop body:

```rust
        view.tokens = view.agents.iter().map(|agent| agent.weighted_tokens).sum();
```

In the existing state test `a_done_agent_without_an_end_time_is_dropped`, add `weighted_tokens: 0,` to its `AgentState` literal. Let the compiler list any other literal.

- [ ] **Step 14: Run the Rust checks to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml tokens`
Expected: 5 tests pass (3 in `events`, 2 in `state`).

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: all tests pass.

Run: `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: no errors, and no `unwrap` in the new code.

- [ ] **Step 15: Write the failing page tests**

In `tools/factory-server/tests/board.test.js`, append:

```js
test("formatTokens scales to k and M and tolerates bad input", () => {
  assert.equal(board.formatTokens(0), "0");
  assert.equal(board.formatTokens(950), "950");
  assert.equal(board.formatTokens(12400), "12k");
  assert.equal(board.formatTokens(999999), "999k");
  assert.equal(board.formatTokens(2170000), "2.17M");
  assert.equal(board.formatTokens(Number.NaN), "0");
  assert.equal(board.formatTokens(-5), "0");
});

test("tokensLine shows weighted tokens only when the job has some", () => {
  assert.match(board.tokensLine(makeJob({ tokens: 2170000 })), /tokens 2\.17M weighted/);
  assert.equal(board.tokensLine(makeJob({})), "");
  assert.equal(board.tokensLine(makeJob({ tokens: 0 })), "");
  assert.equal(board.tokensLine(makeJob({ tokens: "lots" })), "");
});

test("renderCard includes the tokens line", () => {
  assert.match(board.renderCard(makeJob({ tokens: 12400 }), 200), /tokens 12k weighted/);
  assert.doesNotMatch(board.renderCard(makeJob({}), 200), /class="tokens"/);
});
```

- [ ] **Step 16: Run the page tests to verify they fail**

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: the three new tests fail with `board.formatTokens is not a function` (and the card test fails the match).

- [ ] **Step 17: Implement the page change**

In `tools/factory-server/static/board.js`:

Add these functions directly before `function renderCard(job, now) {`:

```js
  function formatTokens(count) {
    const value = Number(count);
    if (!Number.isFinite(value) || value <= 0) return "0";
    if (value < 1000) return String(Math.floor(value));
    if (value < 1000000) return `${Math.floor(value / 1000)}k`;
    return `${(value / 1000000).toFixed(2)}M`;
  }

  function tokensLine(job) {
    const count = Number(job.tokens);
    if (typeof job.tokens !== "number" || !Number.isFinite(count) || count <= 0) return "";
    return (
      `<p class="tokens" title="input + cache creation + output + cache reads / 10, subagents only">` +
      `tokens ${formatTokens(count)} weighted</p>`
    );
  }
```

In `renderCard`, directly before the line that is exactly `      chips +` in the returned expression, add the line `      tokensLine(job) +`. In the `api` object add `formatTokens,` and `tokensLine,` after `formatAge,`.

In `tools/factory-server/static/dashboard.css`, directly after the `.meta { ... }` rule add:

```css
.tokens { margin: 0.3rem 0 0; color: var(--muted-foreground); font-size: 12px; }
```

- [ ] **Step 18: Run the page tests to verify they pass**

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: all tests pass.

- [ ] **Step 19: Commit the dashboard change**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, running its git commands inline with no helper agents and no amend step. Suggested message: `feat(factory): show weighted tokens per job on the dashboard`. Then:

```bash
git add tools/factory-server/src/model.rs tools/factory-server/src/events.rs tools/factory-server/src/state.rs tools/factory-server/static/board.js tools/factory-server/static/dashboard.css tools/factory-server/tests/board.test.js && git commit --only -m "<message from the skill>" -- tools/factory-server/src/model.rs tools/factory-server/src/events.rs tools/factory-server/src/state.rs tools/factory-server/static/board.js tools/factory-server/static/dashboard.css tools/factory-server/tests/board.test.js
```

#### Part D: doc sync and final verification

- [ ] **Step 20: Sync the design doc**

In `.claude/software-factory.md`:

(a) Insert this section directly above the line `## Open items`:

```markdown
## Token efficiency (plan 07)

Job 001 used about 12.75M tokens, 11.76M of them cache reads. Weighted tokens are `input + cache_creation + output + cache_read / 10`, and job 001 was about 2.17M weighted. The orchestrator session was 53.6 percent of the job. The rules below came out of that measurement.

- Diffs stay out of the orchestrator. It writes each diff by shell redirect, together with a `.stat` file (`git diff --stat`) and a `.names` file (`git diff --name-only`), and never Reads or pastes the diff.
- `factory-ctl diff-size --stat-file <file>` compares insertions plus deletions with `diff_cap_changed_lines` (default 4000). Over the cap the job goes to `needs-human` with "diff too large, split the plan".
- `factory-ctl classify-diff --names-file <file>` prints `doc-only` when every path is markdown or under `docs/` or `.claude/`, and none is `Cargo.toml`, `package.json`, `tauri.conf.json`, a `settings.json` file or under a `hooks` folder. An empty list is `code`. For a doc-only diff the security reviewer is recorded as `skipped` (which counts as pass), unless its last verdict was CHANGES.
- Reviewers page the diff with offset and limit 1500, because `Read` stops at 2000 lines, and keep replies at most 60 lines. The approver reads the `.stat` file and the review reports, and opens hunks only for the paths its risk rules flag.
- The builder commits with the commit skill but runs its git commands inline. No helper agents, no amend step (one commit per task is one logical change), and a plain conventional message if the skill insists on helpers. At most 2 helper agents per task and none for a commit. The builder never uses `cd` out of its worktree or heredoc appends in Bash (the sandbox refuses them), and uses the Edit and Write tools.
- The spec writer, code reviewer and UI reviewer no longer read `CLAUDE.md` and the rule files, which are already in their context (about 5.5k tokens each). Crate `CLAUDE.md` files are not preloaded and are still read.
- Orchestrator hygiene: run `/factory` in a fresh or lean session, act on the last lines of agent replies, never paste a diff, and when a background builder has had no events for 10 minutes, ask the user.
- The `log-event` hook adds a `tokens` object (`input`, `cache_read`, `cache_creation`, `output`, `weighted`) to every `SubagentStop` event by streaming `agent_transcript_path`, one usage per unique message id. It reads at most 20 MB, exits 0 on any error and never blocks.
- `factory-ctl tokens <id>` sums the weighted tokens of the agents of a job (largest total per agent id, because a resumed agent reports a cumulative total). `factory-ctl budget-check <id>` prints `ok`, `warn` (80 percent) or `stop` (100 percent) against `job_budget_weighted` (default 3,000,000). `/factory` runs it before each dispatch and parks the job in `needs-human` with "token budget" on `stop`. `factory-ctl status` and the dashboard card show the total. All numbers are in `.claude/factory/config.json`.
- The attribution is approximate. An event belongs to a job when its cwd is under the job worktree, when it is the recorded builder, or when a factory agent that runs from the checkout (spec writer, reviewers, approver) stopped while the job sat in the spec, review or approve stage. The orchestrator's own tokens are not counted, because only subagents write `SubagentStop` events. The dashboard attributes agents with its own rules, so the two totals can differ a little.
```

(b) In the "Live tracking dashboard" section, in the bullet that starts `- Stage comes from` replace the sentence `No cost tracking in v1.` with `Each card shows the weighted tokens of its subagents. No money cost.` If an earlier plan already changed that sentence, keep its wording and add `Each card shows the weighted tokens of its subagents.` to the bullet.

(c) In the Agents table, in the `factory-builder` row, add to the Rocket duties cell: `Commits with the skill's git commands inline (no helper agents, no amend), at most 2 helper agents per task, never \`cd\` out of the worktree or heredoc appends.`

- [ ] **Step 21: Final verification**

Run each command and confirm the expected result:

```bash
bash .claude/factory/tests/run.sh | tail -2
python3 -m py_compile .claude/factory/factory-ctl .claude/factory/hooks/log-event && echo compiled
cargo test -j4 --manifest-path tools/factory-server/Cargo.toml
cargo check -j4 --manifest-path tools/factory-server/Cargo.toml
node --test tools/factory-server/tests/board.test.js
jq -e '.job_budget_weighted == 3000000 and .budget_warn_percent == 80 and .diff_cap_changed_lines == 4000' .claude/factory/config.json
git status --short
```

Expected: `all passed`; `compiled`; all Rust tests pass; `cargo check` has no errors; all page tests pass; `jq` prints `true`; `git status --short` lists only `.claude/software-factory.md` as modified (everything else is committed). Delete any `__pycache__` folder that `py_compile` left under `.claude/factory/`, because it must not be committed.

- [ ] **Step 22: Spec coverage check**

Confirm by reading the files that each gap has its fix, and fix any miss inline before the commit. F1: the `## Diff hygiene` and `## Context hygiene` sections of `factory.md`. F2: builder commit bullet. F3: plan 02 already bans sleeping, and the stall note covers the orchestrator side. F4: redirect, `diff-size` cap, paging text in the four reviewers and the approver. F5: `classify-diff` and `security=skipped`. F6: the trimmed read lists. F7: plan 02 and plan 04 handle the builder skill text, and this plan's token logging makes the cost visible. F8: `tokens`, `budget-check`, the helper cap and the stall note (the builder `maxTurns` came from plan 02). F9: the `cd` and heredoc rule in the builder prompt and in `factory.md`. Decision 11 items (a) to (h) map to the same places.

- [ ] **Step 23: Commit the doc**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, running its git commands inline with no helper agents and no amend step. Suggested message: `docs(factory): document token efficiency rules and the job budget`. Then:

```bash
git add .claude/software-factory.md && git commit --only -m "<message from the skill>" -- .claude/software-factory.md
```

---

## Self-review notes

- Spec section F: F1 (Task 1 and 2 hygiene text), F2 (Task 2 builder bullet), F3 (stall note in Task 2, ban from plan 02), F4 (Task 1 redirect, cap, paging), F5 (Task 1 classify and skip), F6 (Task 2 trims), F7 (visibility through Task 3 logging, the skill trimming itself lives in plans 02 and 04), F8 (Task 3 tokens and budget, Task 2 helper cap), F9 (Task 2 `cd` and heredoc rule). Decision 11 (a) to (h) are all covered.
- Names used across tasks: `load_config`, `CONFIG_DEFAULTS`, `classify-diff`, `diff-size`, `tokens`, `budget-check`, `job_budget_weighted`, `budget_warn_percent`, `diff_cap_changed_lines`, `weighted_tokens` (agent), `tokens` (job view and event), `formatTokens`, `tokensLine`, `FACTORY_CONFIG`, `prompt_lacks`.
- The attribution and the missing orchestrator tokens are stated as approximate in the context, in Task 3's docstring and in the doc.

Next plan: none (series complete)
