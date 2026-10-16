# Factory Cost Report, Plan 03: the dashboard cost panel and estimate lines

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The dashboard shows a cost panel (figures per size group and the role split) and, on each card, an estimate line before the job merges and an actual-versus-estimate line after.

**Architecture:** `board.js` gets pure render functions (testable with `node --test`) for the panel and the lines. `app.js` renders the panel into a new `#costs` section with the existing patcher and sets the role bar widths through the CSSOM. Styling reuses the dashboard's tokens. The data comes from `costs` and each job's `estimate` in `/api/state` (plan 02).

**Tech Stack:** Plain JavaScript (no build step), CSS, `node:test`, bash.

**Spec:** `docs/superpowers/specs/2026-10-16-factory-cost-report-design.md` (Design 4 and Testing). Plans 01 and 02 are merged on this branch first.

## Global Constraints

- The dashboard follows its own design system, see the comment at the top of `static/dashboard.css`: one monospace face, cool-grey oklch tokens, hairline borders instead of shadows, 10px panels, pill badges, green and red only for pass and fail. Add no fonts, no new colors, no gradients, no shadows, no animation. Use the existing tokens (`--primary`, `--attention`, `--muted-foreground`, `--input`, `--untested`, `--muted`, `--border`, `--card`). Role colors must never rely on color alone: every segment has a text legend with its percent.
- The CSP is `default-src 'self'`: no inline `style` attributes and no inline scripts. Bar widths go through the CSSOM in `app.js` like the token bar.
- Everything that goes into HTML text or attributes passes `escapeHtml`. Role and group names come from a file and are untrusted. Class names are chosen from fixed lists, never built from file text.
- The page works with the keyboard, at 400 px wide with a side gutter of at least 16 px, and in light and dark themes (tokens only). A table wider than the screen scrolls inside its own container.
- Writing copy: sentence case, plain words. An empty state says what to do next. No emojis. No dollar amounts.
- Token figures use the existing `formatTokens` in `board.js` (`850`, `280K`, `1.2M`).
- Commands: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`, `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` (never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`).
- Commit steps are for a human running the plan by hand: pathspec commit only, message through the skill `dev-workflow-skills:1-git-commit`, never `git add -A` or `git add .`.

## Review Focus

1. A group or role name with markup (`<img onerror=x>`) must show as text, and an unknown role must not create a class from file text. Pinned in Task 1.
2. `costs` is `null`, has no groups, has a group with `n` 0, or a role split that sums to 0: the panel shows the empty state and never throws. Pinned in Task 1.
3. A card with no `estimate` shows nothing, a merged card with an estimate but no token figure shows only the estimate, and the comparison words (within, above, below) match the numbers at the range edges. Pinned in Task 1.
4. The one-second repaint must keep the role bar widths: the patcher replaces nodes, so `applyBarWidths` must run after every render and cover the new segments. Pinned in Task 2.
5. At 400 px the cost table scrolls inside its container and the page body does not scroll sideways. Pinned in Task 3 (manual) and by the CSS rule in Task 2.

## File Structure

```
tools/factory-server/static/board.js        renderCostPanel, estimateLine, roleBar and the card hook
tools/factory-server/static/app.js          render the panel, bar widths
tools/factory-server/static/dashboard.html  the #costs section
tools/factory-server/static/dashboard.css   panel, table, role bar, estimate line
tools/factory-server/tests/board.test.js    render tests
tools/factory-server/tests/seed-demo.sh     demo costs.json and estimates
tools/factory-server/tests/smoke.sh         state checks
tools/factory-server/README.md              test notes
.claude/software-factory.md                 documentation
```

---

### Task 1: Render functions and the card lines

**Files:**
- Modify: `tools/factory-server/static/board.js`
- Modify: `tools/factory-server/tests/board.test.js`

**Interfaces:**
- Consumes: existing `escapeHtml`, `formatTokens`, `formatAge`, `copyChip`; job fields `estimate` (`group`, `median`, `low`, `high`, `n`, `n_approx`) and `token_view.weighted`.
- Produces (exported on `FactoryBoard`):
  - `renderCostPanel(costs, now) -> string` (HTML of the panel content, including its `h2`).
  - `estimateLine(job) -> string` (HTML, empty when there is nothing to say).
  - `roleBar(roles) -> string` (HTML of the stacked bar and its legend, empty without roles).
  - `renderCard` includes `estimateLine(job)` right after `planSection(job)` for every stage.

- [ ] **Step 1: Write the failing tests**

Append to `tools/factory-server/tests/board.test.js`:

```js
const costs = {
  generated_at: 100,
  groups: {
    small: { n: 4, n_approx: 4, median: 280000, low: 220000, high: 370000, roles: { builder: 62 }, corrected: null },
    medium: { n: 2, n_approx: 1, median: 300000, low: 250000, high: 350000, roles: {}, corrected: { n: 1, median: 350000, low: 350000, high: 350000 } },
    all: { n: 6, n_approx: 5, median: 290000, low: 220000, high: 370000, roles: { builder: 62 }, corrected: null },
  },
  roles: { builder: 62, committer: 20, reviewer: 10, message: 8 },
};

test("the cost panel lists a row per group in order with median, range and notes", () => {
  const html = board.renderCostPanel(costs, 160);
  assert.match(html, /<h2[^>]*>Cost per job<\/h2>/);
  const order = ["small", "medium", "all"].map((g) => html.indexOf(`>${g}<`));
  assert.ok(order[0] > 0 && order[0] < order[1] && order[1] < order[2], html);
  assert.doesNotMatch(html, />complex</);
  assert.match(html, /280K/);
  assert.match(html, /220K to 370K/);
  assert.match(html, /4 of 4 approximate/);
  assert.match(html, /1 of 2 approximate/);
  assert.match(html, /few jobs/);
  assert.match(html, /updated 1m ago/);
});

test("approximate medians carry a tilde with an explanation", () => {
  const html = board.renderCostPanel(costs, 160);
  assert.match(html, /280K <span class="approx" title="[^"]+">~<\/span>/);
});

test("corrected figures are listed when a group has them", () => {
  const html = board.renderCostPanel(costs, 160);
  assert.match(html, /Exact counts only/);
  assert.match(html, /medium: median 350K \(1 job\)/);
});

test("the role bar has a segment and a legend entry per known role", () => {
  const html = board.roleBar({ builder: 62, committer: 20, reviewer: 10, message: 8 });
  assert.equal((html.match(/class="role-seg role-(builder|committer|reviewer|message)" data-pct="\d+"/g) || []).length, 4);
  assert.match(html, /builder 62%/);
  assert.match(html, /role="img"/);
  assert.match(html, /aria-label="[^"]*builder 62%[^"]*"/);
});

test("an unknown or hostile role name is escaped and gets no class from file text", () => {
  const html = board.roleBar({ '<img onerror=x>': 40, builder: 60 });
  assert.doesNotMatch(html, /<img/);
  assert.match(html, /&lt;img onerror=x&gt; 40%/);
  assert.match(html, /class="role-seg role-other" data-pct="40"/);
  assert.equal(board.roleBar({}), "");
  assert.equal(board.roleBar(null), "");
});

test("group names are escaped and an unknown group is still listed after the known ones", () => {
  const html = board.renderCostPanel({ generated_at: 1, groups: { "<b>x</b>": { n: 1, n_approx: 0, median: 5, low: 5, high: 5, roles: {} } }, roles: {} }, 2);
  assert.doesNotMatch(html, /<b>x/);
});

test("the empty panel invites an action and does not throw on missing data", () => {
  for (const empty of [null, undefined, {}, { groups: {}, roles: {} }, { groups: { small: { n: 0 } }, roles: {} }]) {
    const html = board.renderCostPanel(empty, 10);
    assert.match(html, /No cost history yet/);
    assert.match(html, /data-copy="factory costs"/);
  }
});

test("an unmerged card with an estimate shows the range, group and caveats", () => {
  const job = makeJob({ stage: "build", estimate: { group: "small", median: 280000, low: 220000, high: 370000, n: 4, n_approx: 4 } });
  const html = board.renderCard(job, 200, []);
  assert.match(html, /<p class="estimate">Estimated 220K to 370K weighted \(small, 4 jobs, 4 approximate\)<\/p>/);
  const few = makeJob({ stage: "spec", estimate: { group: "all", median: 10, low: 10, high: 10, n: 1, n_approx: 0 } });
  assert.match(board.renderCard(few, 200, []), /Estimated 10 to 10 weighted \(all sizes, 1 job, few jobs\)/);
});

test("a card without an estimate shows no estimate line", () => {
  const html = board.renderCard(makeJob({ stage: "build" }), 200, []);
  assert.doesNotMatch(html, /class="estimate"/);
  assert.equal(board.estimateLine(makeJob({})), "");
  assert.equal(board.estimateLine(makeJob({ estimate: "lots" })), "");
});

test("a merged card compares the actual cost with the estimate range", () => {
  const est = { group: "small", median: 280000, low: 220000, high: 370000, n: 4, n_approx: 0 };
  const within = board.estimateLine(makeJob({ stage: "merged", estimate: est, token_view: { weighted: 300000 } }));
  assert.match(within, /Cost 300K, estimate 220K to 370K: within the range/);
  assert.match(board.estimateLine(makeJob({ stage: "merged", estimate: est, token_view: { weighted: 220000 } })), /within the range/);
  assert.match(board.estimateLine(makeJob({ stage: "merged", estimate: est, token_view: { weighted: 370000 } })), /within the range/);
  assert.match(board.estimateLine(makeJob({ stage: "merged", estimate: est, token_view: { weighted: 370001 } })), /above the range/);
  assert.match(board.estimateLine(makeJob({ stage: "merged", estimate: est, token_view: { weighted: 100 } })), /below the range/);
  const noActual = board.estimateLine(makeJob({ stage: "merged", estimate: est }));
  assert.match(noActual, /Estimated 220K to 370K/);
  assert.doesNotMatch(noActual, /Cost /);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: FAIL, `board.renderCostPanel is not a function` and the like.

- [ ] **Step 3: Implement**

In `board.js`, above `function renderCard`, add:

```js
  const GROUP_ORDER = ["small", "medium", "complex", "all"];
  const GROUP_LABEL = { all: "all" };
  const ROLE_CLASS = ["builder", "committer", "reviewer", "message", "kb"];
  const FEW_JOBS = 3;

  function jobsWord(n) {
    return n === 1 ? "1 job" : `${n} jobs`;
  }

  // The words that tell a reader how far to trust a figure.
  function estimateNotes(group, n, nApprox) {
    const notes = [group === "all" ? "all sizes" : group, jobsWord(n)];
    if (n < FEW_JOBS) notes.push("few jobs");
    if (nApprox > 0) notes.push(`${nApprox} approximate`);
    return notes.join(", ");
  }

  function isEstimate(e) {
    return Boolean(e) && typeof e === "object" && [e.median, e.low, e.high, e.n].every((v) => Number.isFinite(Number(v)));
  }

  // One line on a card: the estimate before the job merges, the actual cost against it afterwards.
  function estimateLine(job) {
    const e = job && job.estimate;
    if (!isEstimate(e)) return "";
    const range = `${formatTokens(e.low)} to ${formatTokens(e.high)}`;
    const actual = job.token_view && Number(job.token_view.weighted);
    if (job.stage === "merged" && Number.isFinite(actual) && actual > 0) {
      const word = actual > e.high ? "above" : actual < e.low ? "below" : "within";
      return `<p class="estimate">Cost ${formatTokens(actual)}, estimate ${range}: ${word} the range</p>`;
    }
    return `<p class="estimate">Estimated ${range} weighted (${escapeHtml(estimateNotes(String(e.group), Number(e.n), Number(e.n_approx) || 0))})</p>`;
  }

  function roleClass(role) {
    return ROLE_CLASS.includes(role) ? `role-${role}` : "role-other";
  }

  // A stacked bar of the role shares and a text legend. Widths are set from data-pct by app.js.
  function roleBar(roles) {
    const entries = Object.entries(roles || {}).filter(([, pct]) => Number(pct) > 0);
    if (!entries.length) return "";
    const label = entries.map(([role, pct]) => `${role} ${Number(pct)}%`).join(", ");
    const segs = entries
      .map(([role, pct]) => `<span class="role-seg ${roleClass(role)}" data-pct="${Math.max(0, Math.min(100, Number(pct)))}"></span>`)
      .join("");
    const legend = entries
      .map(([role, pct]) => `<li><span class="role-key ${roleClass(role)}" aria-hidden="true"></span>${escapeHtml(role)} ${Number(pct)}%</li>`)
      .join("");
    return (
      `<div class="role-bar" role="img" aria-label="${escapeHtml(`Share of tokens by role: ${label}`)}">${segs}</div>` +
      `<ul class="role-legend">${legend}</ul>`
    );
  }

  function costRow(name, g) {
    const approx = Number(g.n_approx) > 0;
    const notes = [];
    if (Number(g.n) < FEW_JOBS) notes.push("few jobs");
    if (approx) notes.push(`${Number(g.n_approx)} of ${Number(g.n)} approximate`);
    const mark = approx
      ? ` <span class="approx" title="Output tokens were counted too low for some of these jobs, so the figure is a little low.">~</span>`
      : "";
    return (
      `<tr><th scope="row">${escapeHtml(GROUP_LABEL[name] || name)}</th><td>${Number(g.n)}</td>` +
      `<td>${formatTokens(g.median)}${mark}</td><td>${formatTokens(g.low)} to ${formatTokens(g.high)}</td>` +
      `<td class="cost-notes">${escapeHtml(notes.join(", "))}</td></tr>`
    );
  }

  function renderCostPanel(costs, now) {
    const groups = (costs && costs.groups) || {};
    const names = [...GROUP_ORDER.filter((g) => groups[g]), ...Object.keys(groups).filter((g) => !GROUP_ORDER.includes(g))];
    const rows = names.filter((name) => groups[name] && Number(groups[name].n) > 0);
    if (!rows.length) {
      return (
        `<h2>Cost per job</h2><p class="cost-empty">No cost history yet. Run ${copyChip("factory costs")} after a job merges, ` +
        `or wait for the next merge.</p>`
      );
    }
    const age =
      costs && Number(costs.generated_at) > 0 && Number.isFinite(Number(now))
        ? `<span class="cost-updated">updated ${escapeHtml(formatAge(Math.max(0, Number(now) - Number(costs.generated_at))))} ago</span>`
        : "";
    const corrected = rows
      .filter((name) => groups[name].corrected && Number(groups[name].corrected.n) > 0)
      .map((name) => {
        const c = groups[name].corrected;
        return `<li>${escapeHtml(GROUP_LABEL[name] || name)}: median ${formatTokens(c.median)} (${jobsWord(Number(c.n))})</li>`;
      });
    return (
      `<h2>Cost per job ${age}</h2>` +
      `<div class="cost-scroll"><table class="cost-table"><caption class="sr-only">Weighted tokens per finished job, by plan size</caption>` +
      `<thead><tr><th scope="col">size</th><th scope="col">jobs</th><th scope="col">median</th><th scope="col">range</th><th scope="col">notes</th></tr></thead>` +
      `<tbody>${rows.map((name) => costRow(name, groups[name])).join("")}</tbody></table></div>` +
      (corrected.length ? `<p class="cost-note">Exact counts only</p><ul class="cost-corrected">${corrected.join("")}</ul>` : "") +
      `<h3 class="cost-sub">Where the tokens go</h3>${roleBar(costs && costs.roles)}`
    );
  }
```

In `renderCard`, add `estimateLine(job) +` right after `planSection(job) +`. In the exported `api` object add `renderCostPanel, estimateLine, roleBar,` next to `copyChip`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`
Expected: all pass, including every earlier card test (a card without an `estimate` renders as before).

- [ ] **Step 5: Commit**

Commit `board.js` and `board.test.js` with the skill message `feat(factory-server): render the cost panel and estimate lines`.

---

### Task 2: The #costs section, bar widths and styles

**Files:**
- Modify: `tools/factory-server/static/dashboard.html`
- Modify: `tools/factory-server/static/app.js`
- Modify: `tools/factory-server/static/dashboard.css`
- Modify: `tools/factory-server/src/server.rs` (embed test)

**Interfaces:**
- Consumes: Task 1 render functions, `view.costs` and `view.generated_at` in the state.
- Produces: the `#costs` section, role bar widths applied after every render, panel styles.

- [ ] **Step 1: Write the failing check**

Add to the tests module of `tools/factory-server/src/server.rs`:

```rust
    #[test]
    fn the_page_carries_the_cost_panel_and_its_script_and_styles() -> TestResult {
        let (_dir, app) = app_with_board(Some(JOB))?;
        let host = Some("127.0.0.1:1");
        let html = String::from_utf8(route(&app, "GET", "/", host, 1, 5.0).body)?;
        assert!(html.contains(r#"id="costs""#), "costs section");
        let app_js = String::from_utf8(route(&app, "GET", "/app.js", host, 1, 5.0).body)?;
        assert!(app_js.contains("renderCostPanel") && app_js.contains(".role-seg"), "controller");
        let css = String::from_utf8(route(&app, "GET", "/dashboard.css", host, 1, 5.0).body)?;
        assert!(css.contains(".cost-table") && css.contains(".role-bar") && css.contains(".estimate"), "styles");
        Ok(())
    }
```

- [ ] **Step 2: Run the check to verify it fails**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml the_page_carries_the_cost_panel`
Expected: FAIL on the first assert.

- [ ] **Step 3: Implement**

`dashboard.html`: add the section between the board and the `lower` section:

```html
    <section id="costs" class="panel costs" aria-label="Cost per job"></section>
```

(place it right after the closing tag of `<div class="board-scroll">...</div>` and before `<section class="lower">`.)

`app.js`: in `render()`, after `setHtml("board", ...)` add `setHtml("costs", FactoryBoard.renderCostPanel(view.costs, view.generated_at));`, and change `applyBarWidths` so it covers both bars:

```js
  function applyBarWidths() {
    for (const fill of document.querySelectorAll(".tokens-fill, .role-seg")) {
      const width = `${fill.getAttribute("data-pct") || 0}%`;
      if (fill.style.width !== width) fill.style.width = width;
    }
  }
```

`dashboard.css`: append (existing tokens only):

```css
/* The cost panel and the estimate line. */
.costs { margin-top: 0.75rem; }
.costs h2 { display: flex; flex-wrap: wrap; gap: 0.25rem 0.75rem; align-items: baseline; }
.cost-updated { color: var(--muted-foreground); font-weight: 400; }
.cost-scroll { overflow-x: auto; }
.cost-table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
.cost-table th, .cost-table td { padding: 0.35rem 0.75rem 0.35rem 0; border-bottom: 1px solid var(--border); text-align: right; white-space: nowrap; }
.cost-table th:first-child, .cost-table td:first-child, .cost-table th:last-child, .cost-table td:last-child { text-align: left; }
.cost-table thead th { color: var(--muted-foreground); font-weight: 500; }
.cost-table tbody th { font-weight: 600; }
.cost-notes { color: var(--muted-foreground); }
.approx { color: var(--attention); cursor: help; }
.cost-note { margin: 0.7rem 0 0.2rem; color: var(--muted-foreground); font-size: 12px; }
.cost-corrected { margin: 0; padding-left: 1rem; font-size: 12.5px; }
.cost-sub { margin: 0.9rem 0 0.4rem; color: var(--muted-foreground); font-size: 11.5px; font-weight: 500; }
.cost-empty { margin: 0; color: var(--muted-foreground); }
.role-bar { display: flex; height: 0.5rem; border-radius: 999px; background: var(--muted); overflow: hidden; }
.role-seg { display: block; height: 100%; border-right: 1px solid var(--card); }
.role-legend { display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; margin: 0.5rem 0 0; padding: 0; list-style: none; font-size: 12px; }
.role-key { display: inline-block; width: 0.6rem; height: 0.6rem; margin-right: 0.35rem; border-radius: 2px; }
.role-builder { background: var(--primary); }
.role-committer { background: var(--attention); }
.role-reviewer { background: var(--muted-foreground); }
.role-message { background: var(--input); }
.role-kb, .role-other { background: var(--untested); }
.estimate { margin: 0.35rem 0 0; color: var(--muted-foreground); font-size: 12px; }
```

- [ ] **Step 4: Run the checks to verify they pass**

Run: `node --check tools/factory-server/static/app.js`, `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`, then `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`.
Expected: all pass. There is no browser in this environment, so state that in the report.

- [ ] **Step 5: Commit**

Commit `dashboard.html`, `app.js`, `dashboard.css` and `server.rs` with the skill message `feat(factory-server): show the cost panel on the dashboard`.

---

### Task 3: Demo data, smoke checks and documentation

**Files:**
- Modify: `tools/factory-server/tests/seed-demo.sh`
- Modify: `tools/factory-server/tests/smoke.sh`
- Modify: `tools/factory-server/README.md`
- Modify: `.claude/software-factory.md`

**Interfaces:**
- Consumes: everything above.
- Produces: a demo root with a cost panel and estimates, a smoke check of the state, and the docs.

- [ ] **Step 1: Seed the demo**

In `seed-demo.sh`, after the park.json block for job 003, add:

```bash
# A cost summary and estimates, for a look at the panel and the card lines.
cat >"$data/costs.json" <<JSON
{"generated_at":$((now - 240)),"thresholds":{"small_max_bytes":12288,"medium_max_bytes":25600},
 "groups":{
  "small":{"n":4,"n_approx":4,"median":280000,"low":220000,"high":370000,"roles":{"builder":61,"committer":22,"reviewer":11,"message":6},"corrected":null},
  "medium":{"n":2,"n_approx":1,"median":300000,"low":250000,"high":350000,"roles":{"builder":64,"committer":20,"reviewer":10,"message":6},"corrected":{"n":1,"median":350000,"low":350000,"high":350000}},
  "complex":{"n":3,"n_approx":3,"median":450000,"low":420000,"high":480000,"roles":{"builder":70,"committer":15,"reviewer":10,"message":5},"corrected":null},
  "all":{"n":9,"n_approx":8,"median":310000,"low":220000,"high":480000,"roles":{"builder":65,"committer":19,"reviewer":10,"message":6},"corrected":{"n":1,"median":350000,"low":350000,"high":350000}}},
 "roles":{"builder":65,"committer":19,"reviewer":10,"message":6}}
JSON
sed -e "s/{\"id\":\"001\",/{\"id\":\"001\",\"estimate\":{\"group\":\"small\",\"median\":280000,\"low\":220000,\"high\":370000,\"n\":4,\"n_approx\":4},/" \
    -e "s/{\"id\":\"004\",/{\"id\":\"004\",\"estimate\":{\"group\":\"medium\",\"median\":300000,\"low\":250000,\"high\":350000,\"n\":2,\"n_approx\":1},/" \
    -e "s/{\"id\":\"005\",/{\"id\":\"005\",\"estimate\":{\"group\":\"all\",\"median\":310000,\"low\":220000,\"high\":480000,\"n\":1,\"n_approx\":0},/" \
    "$data/board.json" >"$data/board.json.tmp"
mv "$data/board.json.tmp" "$data/board.json"
```

- [ ] **Step 2: Add the smoke checks**

In `smoke.sh`, before the final `if [ "$fails" -eq 0 ]` block, add:

```bash
# The cost summary and a job estimate reach the state, the per-job list does not.
cat >"$data/costs.json" <<JSON
{"generated_at":$now,"groups":{"small":{"n":2,"n_approx":1,"median":280000,"low":220000,"high":340000,"roles":{"builder":60},"corrected":null}},"roles":{"builder":60},"jobs":[{"id":"SECRET-JOB"}]}
JSON
await "the state carries the group summary" '.costs | select(.groups.small.n==2 and .roles.builder==60)'
check "the state never carries a per-job list from costs.json" bash -c "! curl -fs $url/api/state | grep -q SECRET-JOB"
cat >"$board" <<JSON
{"jobs":[{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"build","round":0,"stage_since":1,"reviews":{},"estimate":{"group":"small","median":280000,"low":220000,"high":370000,"n":4,"n_approx":2}}]}
JSON
await "a job estimate passes through to the state" '.jobs[0].estimate | select(.group=="small" and .median==280000)'
echo "garbage" >"$data/costs.json"
await "a broken costs.json gives costs null and no board error" '.costs == null and .board_error == null'
```

(If the script defines `data` and `board` earlier, as it does, reuse them. The `await` helper takes a description and a jq filter and is already defined in the script.)

- [ ] **Step 3: Documentation**

In `tools/factory-server/README.md`, after the lines added for the park endpoint, add: `The smoke test also checks that costs.json reaches /api/state as a group summary without any per-job list, that a job estimate passes through, and that a broken costs.json gives costs null.` and after the seed-demo line: `The demo also has a costs.json and estimates on jobs 001, 004 and 005, so the cost panel and the estimate lines show.`

In `.claude/software-factory.md`, in the Token baseline section after the paragraph added for sub-project A, add:

`Cost report: jobs are grouped by plan size, from the bytes of plan.md: small is under size_small_max_bytes (12288), medium is up to size_medium_max_bytes (25600), complex is above. Both are config keys and invalid values fail closed. factory costs prints every job (stage, group, tasks, changed lines, weighted and raw tokens, runs; a ~ marks jobs whose usage.jsonl has lines without usage_v 2, so their output is under-counted), then a table per group (jobs, median, range, few jobs under 3 jobs, approximate count), the figures over exact jobs only, and the role split. It writes .superpowers/factory/costs.json with the group figures and the role split (not the per-job list). The driver rebuilds costs.json after every merge, best effort. Only merged jobs with data feed the figures. factory new stores an estimate on every job it makes from a plan (group, median, low, high, n, n_approx) from the history at that moment, falling back to all sizes when the group has no jobs, and prints it.`

and in the Dashboard section a bullet:

`- The cost panel below the board reads costs.json through /api/state (costs: groups and roles only, null when the file is missing or broken): a row per size group with jobs, median (~ when some jobs are approximate), range and notes, the exact-counts-only medians, and the role split as a stacked bar with a text legend. A card that is not merged shows its estimate; a merged card shows its cost against the estimate range (within, above or below).`

- [ ] **Step 4: Verify**

Run, one per call: `bash tools/factory-server/tests/smoke.sh` (expect `all passed`), `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`, `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`, and `bash tools/factory-server/tests/seed-demo.sh /tmp/claude-1000/factory-demo` (expect `Demo root ready`; then `grep -c '"estimate"' /tmp/claude-1000/factory-demo/.superpowers/factory/board.json` prints 3 and `test -f /tmp/claude-1000/factory-demo/.superpowers/factory/costs.json` succeeds).

Manual look (left to the user, agents cannot see a browser): start the server with the command the seed script prints, open the page, check the panel in light and dark themes, at phone width (the table scrolls inside its box and the page does not), the role bar widths staying put across the one-second repaint, and the estimate lines on cards 001, 004 and 005.

- [ ] **Step 5: Commit**

Commit `seed-demo.sh`, `smoke.sh`, `README.md` and `.claude/software-factory.md` with the skill message `docs(factory): document the cost report and extend the smoke test`.

Next plan: none
