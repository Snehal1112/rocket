# Factory Spec Cards, Plan 03: Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The dashboard shows a `spec S2` chip on each plan card, a slim card for a plan that has not started, and one spec card per series with a "Plans n/N done" list, all in the right columns.

**Architecture:** `board.js` stays one file with pure render functions that return HTML strings. `renderCard` gets an optional third argument (the `specs[]` list) for the chip title and hides the empty sections of a job at stage `spec`. A new `renderSpecCard` reuses the task row and segment bar markup. `renderBoard` places spec cards by their `stage`. No new CSS system, only a cancelled row style.

**Tech Stack:** Plain JavaScript (CommonJS for node tests, a global in the browser), CSS, `node --test`, bash for the demo seed.

**Spec:** `docs/superpowers/specs/2026-10-13-factory-spec-and-plan-cards-design.md`, section "Page". Plans 01 and 02 must be done: `/api/state` carries `series.id` and `specs[]`.

## Global Constraints

- Test command: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js` (the file form, not the folder form).
- The CSP forbids inline styles and scripts: no `style=` attributes in the markup, no inline handlers.
- Every value from the server goes through `escapeHtml` (or `inlineCode`, which escapes first) before it enters a template.
- A series id, a state or a stage the page does not know falls back to a safe default, and never throws.
- Spec cards are not jobs: `renderSummary` and the job counts do not change.
- Commits: conventional commits. Use the `dev-workflow-skills:1-git-commit` skill for every commit, with a pathspec commit. Never `git add -A`, `--all` or `.`.
- Comments are short full sentences that end with a period.

## Review Focus

- A plan title with HTML in it is escaped in the spec card and in the chip title (Task 1 and 2 tests).
- A plan state the page does not know shows as waiting and does not break the card (Task 2 test).
- A job with no series id, or in a one-plan series, shows no spec chip (Task 1 test).
- An opened or closed "Plans" list stays as the user left it across the one-second redraw, with no clash with a job id (Task 2 test).
- A state from an older server without `specs` still renders the board (Task 2 test).

---

### Task 1: Spec chip and the card of a plan that has not started

**Files:**
- Modify: `tools/factory-server/static/board.js` (`specChip`, `renderCard`, `renderBoard` calls, exports)
- Modify: `tools/factory-server/tests/board.test.js` (tests)

**Interfaces:**
- Consumes: `job.series.id` from the server, `state.specs[]` items `{series_id, title, ...}`.
- Produces: `FactoryBoard.specChip(series, specs) -> string` and `FactoryBoard.renderCard(job, now, specs)` (third argument optional).

- [ ] **Step 1: Write the failing tests**

Add to `tests/board.test.js`:

```js
test("a plan card shows which spec it belongs to", () => {
  const job = makeJob({ stage: "build", series: { index: 2, total: 5, id: "S2" } });
  const html = board.renderCard(job, 200, [{ series_id: "S2", title: "Baseline <b>benches</b>" }]);
  assert.match(html, />spec S2</);
  assert.match(html, /title="Spec S2: Baseline &lt;b&gt;benches&lt;\/b&gt;"/);
  assert.match(html, /plan 2 of 5/);
});

test("the spec chip works without spec titles and is left out for old jobs and one-plan series", () => {
  const chip = board.specChip({ index: 1, total: 3, id: "S9" }, undefined);
  assert.match(chip, />spec S9</);
  assert.match(chip, /title="Spec S9"/);
  assert.equal(board.specChip({ index: 1, total: 3 }, []), "");
  assert.equal(board.specChip({ index: 1, total: 1, id: "S1" }, []), "");
  assert.equal(board.specChip(null, []), "");
});

test("a plan card that has not started hides the empty sections and keeps the task list open", () => {
  const plan = { title: "Collection benches", goal: null, tasks: [{ number: 1, title: "the hook", status: "pending" }] };
  const extra = {
    series: { index: 2, total: 5, id: "S2" },
    plan,
    token_view: { weighted: 1, raw: 1, budget: 10, fraction: 0.1 },
    agents: [{ agent_id: "a", agent_type: "factory-builder", status: "running", tool_count: 1 }],
    worktree: "/tmp/wt",
  };
  const waiting = board.renderCard(makeJob(Object.assign({ stage: "spec" }, extra)), 200);
  assert.doesNotMatch(waiting, /Branch and files/);
  assert.doesNotMatch(waiting, /tokens-text/);
  assert.doesNotMatch(waiting, /Agent activity/);
  assert.match(waiting, /Task 1/);
  assert.match(waiting, /waiting/);
  assert.match(waiting, /<details class="tasks-box"[^>]* open/);
  const running = board.renderCard(makeJob(Object.assign({ stage: "build" }, extra)), 200);
  assert.match(running, /Branch and files/);
  assert.match(running, /tokens-text/);
  assert.match(running, /Agent activity/);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: the three new tests FAIL (`board.specChip is not a function`, missing chip).

- [ ] **Step 3: Write the implementation**

In `board.js`, after `seriesChip`, add:

```js
  // Says which spec a plan belongs to. Jobs made before series ids, and one-plan series, have none.
  function specChip(series, specs) {
    if (!series || !series.id) return "";
    const total = Number(series.total);
    if (!Number.isFinite(total) || total < 2) return "";
    const spec = (specs || []).find((item) => item && item.series_id === series.id);
    const label = spec && spec.title ? `Spec ${series.id}: ${spec.title}` : `Spec ${series.id}`;
    return `<span class="series spec-chip" title="${escapeHtml(label)}">spec ${escapeHtml(series.id)}</span>`;
  }
```

Change the signature `function renderCard(job, now)` to `function renderCard(job, now, specs)`. In it, add after `const showAge = ...`:

```js
    // A plan that has not started has no time, tokens, agents or branch yet, so those sections stay out.
    const parked = job.stage === "spec";
```

In the returned markup, change `seriesChip(job.series) +` to `seriesChip(job.series) + specChip(job.series, specs) +`, and replace the tail of the template so it reads:

```js
      planSection(job) +
      (parked ? "" : durationsLine(job)) +
      (parked ? "" : tokensBar(job)) +
      chips +
      (parked ? "" : agentsSection(job, job.agents || [], now)) +
      (parked ? "" : facts) +
      reason +
      `</article>`
```

In `renderBoard`, pass the list on: both calls `renderCard(job, state.generated_at)` become `renderCard(job, state.generated_at, state.specs)`.

In the `api` object, add `specChip,` after `seriesChip,`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`
Expected: PASS.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `board.js`, `board.test.js`. Suggested message: `feat(factory): show the spec on plan cards and slim waiting cards`.

---

### Task 2: The spec card

**Files:**
- Modify: `tools/factory-server/static/board.js` (`renderSpecCard`, `renderBoard`, exports)
- Modify: `tools/factory-server/static/dashboard.css` (cancelled row style)
- Modify: `tools/factory-server/tests/board.test.js` (tests)

**Interfaces:**
- Consumes: `state.specs[]`, each `{series_id, title, goal, stage, plans: [{job_id, index, title, state}]}` from plan 02. `state` is one of `waiting`, `building`, `review`, `needs-human`, `merged`, `cancelled`.
- Produces: `FactoryBoard.renderSpecCard(spec) -> string`. The task list toggle uses `data-tasks="spec-<series_id>"`, so `app.js` keeps the user's choice in `taskChoice` with no change.

- [ ] **Step 1: Write the failing tests**

Add to `tests/board.test.js`:

```js
function makeSpec(overrides) {
  return Object.assign(
    {
      series_id: "S2",
      title: "Baseline benchmarks",
      goal: "Measure the hot paths.",
      stage: "spec",
      plans: [
        { job_id: "001", index: 1, title: "Harness", state: "merged" },
        { job_id: "002", index: 2, title: "Benches", state: "building" },
        { job_id: "003", index: 3, title: null, state: "waiting" },
      ],
    },
    overrides,
  );
}

test("the spec card shows the id, title, goal, plan rows and the done count", () => {
  const html = board.renderSpecCard(makeSpec());
  assert.match(html, /data-spec="S2"/);
  assert.match(html, />S2</);
  assert.match(html, /Baseline benchmarks/);
  assert.match(html, /Measure the hot paths/);
  assert.match(html, />3 plans</);
  assert.match(html, /1\/3 plans done/);
  assert.match(html, /Plan 1<\/span> Harness/);
  assert.match(html, /Plan 3<\/span>/);
  assert.match(html, /task-done/);
  assert.match(html, /task-active/);
  assert.match(html, /task-pending/);
  assert.match(html, />merged</);
  assert.match(html, />building</);
  assert.match(html, />waiting</);
  assert.equal((html.match(/class="seg seg-/g) || []).length, 3);
});

test("the spec card escapes titles and treats an unknown plan state as waiting", () => {
  const html = board.renderSpecCard(
    makeSpec({
      title: "<img src=x onerror=alert(1)>",
      plans: [{ job_id: "001", index: 1, title: "<script>x</script>", state: "exploding" }],
    }),
  );
  assert.doesNotMatch(html, /<img|<script>/);
  assert.match(html, /&lt;script&gt;/);
  assert.match(html, /task-pending/);
  assert.match(html, />waiting</);
});

test("a cancelled plan row and a needs-human row get their own look", () => {
  const html = board.renderSpecCard(
    makeSpec({
      stage: "needs-human",
      plans: [
        { job_id: "001", index: 1, title: "A", state: "cancelled" },
        { job_id: "002", index: 2, title: "B", state: "needs-human" },
      ],
    }),
  );
  assert.match(html, /task-cancelled/);
  assert.match(html, />cancelled</);
  assert.match(html, />needs human</);
  assert.match(html, /card-needs-human/);
});

test("the plan list is open while the series runs and closed once it merged, unless the user chose", () => {
  assert.match(board.renderSpecCard(makeSpec()), /<details class="tasks-box" data-tasks="spec-S2" open>/);
  assert.doesNotMatch(board.renderSpecCard(makeSpec({ stage: "merged" })), /data-tasks="spec-S2" open/);
  board.taskChoice.set("spec-S2", false);
  assert.doesNotMatch(board.renderSpecCard(makeSpec()), /data-tasks="spec-S2" open/);
  board.taskChoice.delete("spec-S2");
});

test("the board puts a spec card in its stage column and counts it", () => {
  const state = makeState([makeJob({ stage: "spec", id: "002" })], { specs: [makeSpec()] });
  const html = board.renderBoard(state);
  assert.match(html, /column-spec"><h2>Spec<span class="sub">[^<]*<\/span><span class="count">2<\/span>/);
  assert.match(html, /data-spec="S2"/);
  const merged = board.renderBoard(makeState([], { specs: [makeSpec({ stage: "merged" })] }));
  assert.match(merged, /column-merged"><h2>Merged<span class="sub">[^<]*<\/span><span class="count">1<\/span>[\s\S]*data-spec="S2"/);
});

test("a state from an older server without specs still renders the board", () => {
  const html = board.renderBoard(makeState([makeJob({ stage: "build" })]));
  assert.match(html, /column-build/);
  assert.doesNotMatch(html, /data-spec=/);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: the new tests FAIL (`board.renderSpecCard is not a function`).

- [ ] **Step 3: Write the implementation**

In `board.js`, after `tasksBox` (before `agentsSection`), add:

```js
  // How a plan row of the spec card looks. Each state maps to the task row class and the segment class.
  const SPEC_ROW = {
    waiting: { row: "pending", seg: "pending", label: "waiting" },
    building: { row: "active", seg: "active", label: "building" },
    review: { row: "review", seg: "review", label: "in review" },
    "needs-human": { row: "active", seg: "active", label: "needs human" },
    merged: { row: "done", seg: "done", label: "merged" },
    cancelled: { row: "cancelled", seg: "pending", label: "cancelled" },
  };

  function specRow(plan) {
    return Object.prototype.hasOwnProperty.call(SPEC_ROW, plan.state) ? SPEC_ROW[plan.state] : SPEC_ROW.waiting;
  }

  // One card for a spec and its plans. It reuses the task list markup, with plans where tasks would be.
  function renderSpecCard(spec) {
    const plans = spec.plans || [];
    const id = escapeHtml(spec.series_id);
    const stage = STAGES.some((s) => s.key === spec.stage) ? spec.stage : "spec";
    const done = plans.filter((plan) => plan.state === "merged").length;
    const segs = plans.map((plan) => `<span class="seg seg-${specRow(plan).seg}"></span>`).join("");
    const rows = plans
      .map((plan) => {
        const look = specRow(plan);
        const title = plan.title ? ` ${inlineCode(plan.title)}` : "";
        return (
          `<li class="task task-${look.row}"><span class="task-mark" aria-hidden="true"></span>` +
          `<span class="task-title"><span class="task-num">Plan ${Number(plan.index) || 0}</span>${title}</span>` +
          `<span class="task-state">${look.label}</span></li>`
        );
      })
      .join("");
    const key = `spec-${spec.series_id}`;
    const open = (taskChoice.has(key) ? taskChoice.get(key) : stage !== "merged") ? " open" : "";
    const goal = spec.goal ? `<p class="plan-goal">${inlineCode(spec.goal)}</p>` : "";
    return (
      `<article class="card card-${stage}" data-spec="${id}">` +
      `<header class="card-head"><span class="job-id">${id}</span><span class="card-stage">Spec</span>` +
      `<span class="series">${plans.length} ${plural(plans.length, "plan", "plans")}</span></header>` +
      `<p class="feature">${escapeHtml(spec.title || `Spec ${spec.series_id}`)}</p>` +
      `<div class="progress"><span class="bar" role="img" aria-label="${done} of ${plans.length} plans done">${segs}</span>` +
      `<span class="progress-label">${done}/${plans.length} ${plural(plans.length, "plan", "plans")} done</span></div>` +
      `<div class="plan">${goal}` +
      `<details class="tasks-box" data-tasks="${escapeHtml(key)}"${open}>` +
      `<summary>Plans <span class="agents-sum">${done}/${plans.length} done</span></summary>` +
      `<ol class="tasks">${rows}</ol></details></div>` +
      `</article>`
    );
  }
```

In `renderBoard`, replace the column-building `.map` body so spec cards come first and are counted:

```js
    const specs = state.specs || [];
    const columns = STAGES.map((stage) => {
      let items = jobs.filter((job) => job.stage === stage.key);
      const specCards = specs.filter((spec) => spec && spec.stage === stage.key);
      const total = items.length + specCards.length;
      if (stage.key === "merged") {
        items = items
          .slice()
          .sort((a, b) => (b.stage_since || 0) - (a.stage_since || 0))
          .slice(0, MERGED_LIMIT);
      }
      const cards =
        items.length || specCards.length
          ? specCards.map(renderSpecCard).join("") +
            items.map((job) => renderCard(job, state.generated_at, specs)).join("")
          : `<p class="empty">Nothing here</p>`;
      return (
        `<section class="column panel column-${stage.key}">` +
        `<h2>${stage.title}<span class="sub">${stage.sub}</span><span class="count">${total}</span></h2>` +
        cards +
        `</section>`
      );
    });
```

(Keep the `const known = ...` and `const unknown = ...` lines above it as they are. The stray-column `renderCard` call keeps passing `state.specs` from Task 1.)

In the `api` object, add `renderSpecCard,` after `renderCard,`.

In `dashboard.css`, after the `.task-done .task-state` line, add:

```css
.task-cancelled .task-mark { border-color: var(--attention); }
.task-cancelled .task-state { color: var(--attention); }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`
Expected: PASS.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `board.js`, `dashboard.css`, `board.test.js`. Suggested message: `feat(factory): add the spec card with its plan list`.

---

### Task 3: Demo series, smoke check and docs

**Files:**
- Modify: `tools/factory-server/tests/seed-demo.sh` (a 5-plan series)
- Modify: `tools/factory-server/README.md` (one line)
- Modify: `.claude/software-factory.md` (dashboard section and a ruling)

**Interfaces:**
- Consumes: the server and page from plans 02 and Tasks 1 and 2.
- Produces: a demo root that shows the 5-plan series, and the docs.

- [ ] **Step 1: Seed a 5-plan series**

In `seed-demo.sh`, directly after the `hist() { ... }` line, add:

```bash
# A five-plan series S1: plan 1 builds and plans 2 to 5 wait at spec. Plan 1's folder holds the spec.
sdd="$root/.superpowers/sdd"
titles=("" "Harness and fixtures" "Collection benches" "Request benches" "Environment benches" "Report and baseline")
for i in 1 2 3 4 5; do
  dir="$sdd/2026-10-13-$(printf '%03d' $((5 + i)))-demo"
  mkdir -p "$dir"
  {
    printf '# Plan: %s\n\n' "${titles[$i]}"
    for t in 1 2 3; do printf '%s Task %s: Step %s of %s\n\n' '###' "$t" "$t" "${titles[$i]}"; done
  } >"$dir/plan.md"
done
printf '# Spec: Baseline benchmarks\n\n## Goal\n\nMeasure the hot paths before any performance change.\n' \
  >"$sdd/2026-10-13-006-demo/spec.md"
```

Replace the line `printf ']}\n'` (inside the `{ ... } >"$data/board.json.tmp"` block) with:

```bash
  for i in 1 2 3 4 5; do
    id="$(printf '%03d' $((5 + i)))"
    if [ "$i" -eq 1 ]; then stage=build; else stage=spec; fi
    printf ','
    job "$id" "Baseline benchmarks" "$stage" null 0 "$none" "[{\"stage\":\"spec\",\"ts\":$((now - 300))}]" \
      "{\"plan_file\":\"plan-0$i.md\",\"index\":$i,\"total\":5,\"id\":\"S1\"}" null
  done
  printf ']}\n'
```

Change the first comment line of the script to say it also builds a five-plan series:
`# Builds a demo factory root with jobs in every stage and one five-plan series, for a visual check of the dashboard.`

- [ ] **Step 2: Check the seed through the real server**

Run:

```bash
demo="$(mktemp -d)"
bash tools/factory-server/tests/seed-demo.sh "$demo"
cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$demo" --port 8765 &
sleep 8
curl -s http://127.0.0.1:8765/api/state | jq -c '.specs[0] | {series_id, title, stage, rows: [.plans[].state]}'
kill %1
rm -rf "$demo"
```

Expected: `{"series_id":"S1","title":"Baseline benchmarks","stage":"spec","rows":["building","waiting","waiting","waiting","waiting"]}`. If `sleep 8` is too short for the first build, wait for the build and run the `curl` again.

Run: `bash tools/factory-server/tests/smoke.sh`
Expected: every line `PASS`.

- [ ] **Step 3: Update the docs**

In `tools/factory-server/README.md`, change the last line to:

```markdown
`tests/seed-demo.sh <dir>` builds a demo root with a job in every stage and one five-plan series for a visual check.
```

In `.claude/software-factory.md`, in the dashboard section after the line that starts `- A spec card is built`, add:

```markdown
- A plan card shows a `spec S2` chip next to `plan k of N` (hover shows the spec title). A plan that has not started sits in the spec column with every task waiting, and its card leaves out stage times, the token bar, agent activity and the branch box until it runs. The spec card shows "Plans n/N done" as a list that reuses the task rows.
```

Add this ruling at the end of the rulings list in the same file:

```markdown
- Ruling (2026-10-13): the dashboard shows one spec card per series and one card per plan from the start (plans 01 to 03 of `docs/superpowers/plans/2026-10-13-factory-spec-cards/`). The visual check in a real window (dark, light and phone width) is left to the user, because agents cannot see a browser.
```

- [ ] **Step 4: Run all checks**

Run: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`
Expected: PASS.

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: PASS.

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: PASS (the doc lint reads `.claude/software-factory.md`).

The `yarn tsc --noEmit` and `yarn check` gates do not apply: no file under `src/` or `scripts/` changed.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `seed-demo.sh`, `README.md`, `.claude/software-factory.md`. Suggested message: `docs(factory): seed a five-plan series and describe spec cards`.

After this commit, tell the user the browser check is still open: run `bash tools/factory-server/tests/seed-demo.sh <dir>`, start the printed server command, and look at dark, light and phone width.
