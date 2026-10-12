# Software Factory Plan 03: Dashboard

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A live stage-by-stage board in the browser: one column per stage, one card per job, review chips, the agents working now with their current tool, and a live feed.

**Architecture:** Plain static files embedded in the Rust server binary: `dashboard.html`, `dashboard.css`, `board.js` (pure render functions that return HTML strings, tested with Node's built-in test runner) and `app.js` (polls `/api/state` every second and swaps the HTML). No framework, no build step, no external requests.

**Tech Stack:** HTML, CSS, vanilla JavaScript (CommonJS-compatible so Node can test it), Node 22 `node --test`, the plan 02 Rust server.

**Spec:** [.claude/software-factory.md](../../../../.claude/software-factory.md). Plan index: [00-plan-index.md](00-plan-index.md). Plans 01 and 02 must be finished first.

## Global Constraints

- The dashboard is dev tooling outside the Rocket app. Files live only under `tools/factory-server/`. Nothing here is imported by `src/` or `src-tauri/`, and `yarn check` (which only covers `src/` and `scripts/`) is unaffected.
- No external requests: no CDN scripts, no web fonts, no images from the network.
- Every value that comes from `board.json` or the event log (feature text, slug, reason, branch, tool target, last message) goes through `escapeHtml` before it reaches an HTML string. Class names built from data are checked against a whitelist.
- Status is never shown by colour alone: every chip, dot and column carries text.
- Dark theme by default, light theme through `prefers-color-scheme`. Both define the full palette as CSS variables.
- Works at phone width: the board scrolls sideways inside its own container, the page body does not.
- State JSON field names come from plan 02: `generated_at`, `board_error`, `jobs[]` (board fields plus `agents[]` and `seconds_in_stage`), `unassigned_agents[]`, `recent[]`.
- Cargo commands use `-j4 --manifest-path tools/factory-server/Cargo.toml`. No unwrap calls in Rust, tests included.
- Commits use the `dev-workflow-skills:1-git-commit` skill, a pathspec, conventional format.

## Review Focus

- A feature text such as `<img src=x onerror=alert(1)>` must show as text and never run. Tested in Task 1.
- A job with a stage the dashboard does not know must not disappear silently. Tested in Task 1.
- A job with no agents, no reviews or missing optional fields must still render. Tested in Task 1.
- The server going away must keep the last board on screen, marked as stale, and say so in the header. Task 2 manual check and `app.js` logic.
- A long merged history must not grow the page without bound: only the 10 newest merged jobs show. Tested in Task 1.

---

### Task 1: Pure render functions with tests

**Files:**
- Create: `tools/factory-server/static/board.js`
- Create: `tools/factory-server/tests/board.test.js`

**Interfaces:**
- Consumes: the state JSON from plan 02.
- Produces: `FactoryBoard` (a global in the browser, `module.exports` in Node) with `STAGES`, `escapeHtml(value)`, `formatAge(seconds)`, `clock(ts)`, `reviewChip(role, state)`, `agentLine(agent)`, `renderCard(job)`, `renderBoard(state)`, `renderUnassigned(state)`, `renderFeed(state)` and `headerStatus(state, error, lastOkTs)` which returns `{ level: "ok" | "warn" | "error", text }`.

- [ ] **Step 1: Write the failing tests**

`tools/factory-server/tests/board.test.js`:

```js
"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const board = require("../static/board.js");

function makeJob(overrides) {
  return Object.assign(
    {
      id: "001",
      slug: "fix-import",
      created: "2026-10-13",
      feature: "Fix the import bug",
      branch: "factory/001-fix-import",
      stage: "build",
      round: 0,
      reason: null,
      stage_since: 100,
      seconds_in_stage: 75,
      reviews: { security: "pending", ux: "pending", ui: "pending", code: "pending" },
      agents: [],
    },
    overrides,
  );
}

function makeState(jobs, extra) {
  return Object.assign(
    { generated_at: 200, board_error: null, jobs, unassigned_agents: [], recent: [] },
    extra,
  );
}

test("escapeHtml escapes the five dangerous characters and tolerates null", () => {
  assert.equal(board.escapeHtml(`<a href="x">'&'</a>`), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
  assert.equal(board.escapeHtml(null), "");
  assert.equal(board.escapeHtml(undefined), "");
});

test("formatAge", () => {
  assert.equal(board.formatAge(-5), "0s");
  assert.equal(board.formatAge(Number.NaN), "0s");
  assert.equal(board.formatAge(12.9), "12s");
  assert.equal(board.formatAge(180), "3m");
  assert.equal(board.formatAge(3900), "1h 05m");
  assert.equal(board.formatAge(2 * 86400 + 5), "2d");
});

test("clock gives hh:mm:ss", () => {
  assert.match(board.clock(1760000000), /^\d\d:\d\d:\d\d$/);
});

test("reviewChip shows role and state as text and whitelists the class", () => {
  const chip = board.reviewChip("security", "pass");
  assert.match(chip, /chip-pass/);
  assert.match(chip, />sec pass</);
  const odd = board.reviewChip("code", '"><script>');
  assert.match(odd, /chip-pending/);
  assert.doesNotMatch(odd, /<script>/);
});

test("agentLine shows the live tool for a running agent", () => {
  const html = board.agentLine({
    agent_type: "factory-builder",
    status: "running",
    last_tool: "Edit",
    last_target: "src/App.tsx",
    tool_count: 14,
  });
  assert.match(html, /agent-running/);
  assert.match(html, />builder</);
  assert.match(html, /Edit src\/App\.tsx/);
  assert.match(html, /14 tools/);
  assert.match(html, />running</);
});

test("agentLine shows the last message for a finished agent and survives odd status", () => {
  const done = board.agentLine({ agent_type: "factory-ui-reviewer", status: "done", last_message: "VERDICT: APPROVE" });
  assert.match(done, /agent-done/);
  assert.match(done, /VERDICT: APPROVE/);
  const odd = board.agentLine({ agent_type: "x", status: '"><script>' });
  assert.match(odd, /agent-done/);
  assert.doesNotMatch(odd, /<script>/);
});

test("renderCard escapes the feature text", () => {
  const html = board.renderCard(makeJob({ feature: "<img src=x onerror=alert(1)>" }));
  assert.doesNotMatch(html, /<img/);
  assert.match(html, /&lt;img src=x onerror=alert\(1\)&gt;/);
});

test("renderCard shows id, slug, date, branch, round and age", () => {
  const html = board.renderCard(makeJob({ round: 1 }));
  assert.match(html, /001/);
  assert.match(html, /fix-import/);
  assert.match(html, /2026-10-13/);
  assert.match(html, /factory\/001-fix-import/);
  assert.match(html, /round 1\/2/);
  assert.match(html, /1m in stage/);
});

test("renderCard shows review chips only from the review stage on", () => {
  assert.doesNotMatch(board.renderCard(makeJob({ stage: "build" })), /class="chip /);
  const review = board.renderCard(makeJob({ stage: "review", reviews: { security: "pass", ux: "skipped", ui: "skipped", code: "changes" } }));
  assert.match(review, /sec pass/);
  assert.match(review, /ux skipped/);
  assert.match(review, /code changes/);
});

test("renderCard for needs-human shows the reason and the commands for that job", () => {
  const html = board.renderCard(makeJob({ id: "003", stage: "needs-human", reason: "touches the <trust gate>" }));
  assert.match(html, /touches the &lt;trust gate&gt;/);
  assert.match(html, /\/factory approve 003/);
  assert.match(html, /\/factory rework 003 &lt;note&gt;/);
});

test("renderCard copes with missing optional fields", () => {
  const html = board.renderCard({ id: "009", stage: "spec" });
  assert.match(html, /009/);
});

test("renderBoard has the six stage columns in order with counts", () => {
  const state = makeState([makeJob({ stage: "build" }), makeJob({ id: "002", stage: "build" }), makeJob({ id: "003", stage: "spec" })]);
  const html = board.renderBoard(state);
  const titles = [...html.matchAll(/<h2>([A-Za-z ]+)</g)].map((m) => m[1]);
  assert.deepEqual(titles, ["Spec", "Build", "Review", "Approve", "Merged", "Needs human"]);
  assert.match(html, /column-build[\s\S]*?<span class="count">2</);
  assert.match(html, /class="empty"/);
});

test("renderBoard shows only the 10 newest merged jobs", () => {
  const jobs = [];
  for (let i = 1; i <= 12; i += 1) {
    jobs.push(makeJob({ id: String(i).padStart(3, "0"), stage: "merged", stage_since: i }));
  }
  const html = board.renderBoard(makeState(jobs));
  assert.equal((html.match(/<article class="card/g) || []).length, 10);
  assert.match(html, /data-job="012"/);
  assert.doesNotMatch(html, /data-job="001"/);
  assert.match(html, /<span class="count">12</);
});

test("renderBoard reports jobs with an unknown stage", () => {
  const html = board.renderBoard(makeState([makeJob({ stage: "teleporting" })]));
  assert.match(html, /1 job with an unknown stage/);
});

test("renderUnassigned and renderFeed", () => {
  assert.equal(board.renderUnassigned(makeState([])), "");
  const state = makeState([], {
    unassigned_agents: [{ agent_type: "factory-approver", status: "running" }],
    recent: [{ ts: 150, kind: "tool", agent_type: "factory-builder", tool: "Read", target: "CLAUDE.md" }],
  });
  assert.match(board.renderUnassigned(state), /approver/);
  assert.match(board.renderFeed(state), /builder/);
  assert.match(board.renderFeed(state), /Read CLAUDE\.md/);
  assert.match(board.renderFeed(makeState([])), /No agent activity yet/);
});

test("headerStatus", () => {
  assert.equal(board.headerStatus(makeState([]), null, 200).level, "ok");
  assert.match(board.headerStatus(makeState([]), null, 200).text, /^board\.json · \d\d:\d\d:\d\d$/);
  assert.equal(board.headerStatus(null, "HTTP 500", null).text, "server unreachable");
  assert.match(board.headerStatus(null, "boom", 150).text, /^server unreachable · last update \d\d:\d\d:\d\d$/);
  assert.equal(board.headerStatus(null, "boom", 150).level, "error");
  const warn = board.headerStatus(makeState([], { board_error: "board.json is invalid: x" }), null, 200);
  assert.equal(warn.level, "warn");
  assert.equal(warn.text, "board.json is invalid: x");
  assert.equal(board.headerStatus(null, null, null).level, "warn");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test tools/factory-server/tests/`
Expected: failure, `Cannot find module '../static/board.js'`.

- [ ] **Step 3: Write `board.js`**

`tools/factory-server/static/board.js`:

```js
"use strict";

(function (root) {
  const STAGES = [
    { key: "spec", title: "Spec", sub: "spec-writer · opus" },
    { key: "build", title: "Build", sub: "builder · opus" },
    { key: "review", title: "Review", sub: "sec · ux · ui · code" },
    { key: "approve", title: "Approve", sub: "approver · opus" },
    { key: "merged", title: "Merged", sub: "main" },
    { key: "needs-human", title: "Needs human", sub: "approve · rework" },
  ];
  const ROLES = ["security", "ux", "ui", "code"];
  const ROLE_LABEL = { security: "sec", ux: "ux", ui: "ui", code: "code" };
  const REVIEW_STATES = ["pending", "running", "pass", "changes", "skipped"];
  const AGENT_STATUSES = ["running", "stale", "done"];
  const CHIP_STAGES = ["review", "approve", "needs-human", "merged"];
  const MERGED_LIMIT = 10;
  const FEED_LIMIT = 15;
  const ESCAPES = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

  function escapeHtml(value) {
    return String(value === null || value === undefined ? "" : value).replace(
      /[&<>"']/g,
      (char) => ESCAPES[char],
    );
  }

  function truncate(text, max) {
    const value = String(text === null || text === undefined ? "" : text);
    return value.length > max ? `${value.slice(0, max - 1)}…` : value;
  }

  function formatAge(seconds) {
    if (!Number.isFinite(seconds) || seconds < 0) return "0s";
    if (seconds < 60) return `${Math.floor(seconds)}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    if (seconds < 86400) {
      const hours = Math.floor(seconds / 3600);
      const minutes = Math.floor((seconds % 3600) / 60);
      return `${hours}h ${String(minutes).padStart(2, "0")}m`;
    }
    return `${Math.floor(seconds / 86400)}d`;
  }

  function clock(ts) {
    return new Date(ts * 1000).toTimeString().slice(0, 8);
  }

  function shortType(agentType) {
    return String(agentType || "agent").replace(/^factory-/, "");
  }

  function reviewChip(role, state) {
    const safeState = REVIEW_STATES.includes(state) ? state : "pending";
    const label = ROLE_LABEL[role] || "?";
    return `<span class="chip chip-${safeState}" title="${escapeHtml(role)}: ${safeState}">${label} ${safeState}</span>`;
  }

  function agentLine(agent) {
    const status = AGENT_STATUSES.includes(agent.status) ? agent.status : "done";
    let detail = "";
    if (status !== "done" && agent.last_tool) {
      const target = agent.last_target ? ` ${truncate(agent.last_target, 48)}` : "";
      detail = ` · ${agent.last_tool}${target}`;
    } else if (status === "done" && agent.last_message) {
      detail = ` · ${truncate(agent.last_message, 60)}`;
    }
    const count = agent.tool_count ? ` · ${agent.tool_count} tools` : "";
    return (
      `<li class="agent agent-${status}"><span class="dot dot-${status}" aria-hidden="true"></span>` +
      `<span class="agent-kind">${escapeHtml(shortType(agent.agent_type))}</span>` +
      `<span class="agent-status">${status}</span>` +
      `<span class="agent-detail">${escapeHtml(detail + count)}</span></li>`
    );
  }

  function renderCard(job) {
    const id = escapeHtml(job.id);
    const stage = STAGES.some((s) => s.key === job.stage) ? job.stage : "spec";
    const chips = CHIP_STAGES.includes(job.stage)
      ? `<div class="chips">${ROLES.map((role) => reviewChip(role, (job.reviews || {})[role])).join("")}</div>`
      : "";
    const agents = (job.agents || []).map(agentLine).join("");
    const reason =
      job.stage === "needs-human"
        ? `<p class="reason">${escapeHtml(job.reason || "needs a human")}</p>` +
          `<p class="hint"><code>/factory approve ${id}</code> <code>/factory rework ${id} &lt;note&gt;</code></p>`
        : "";
    return (
      `<article class="card card-${stage}" data-job="${id}">` +
      `<header class="card-head"><span class="job-id">${id}</span>` +
      `<span class="job-slug">${escapeHtml(job.slug)}</span>` +
      `<time class="job-date">${escapeHtml(job.created)}</time></header>` +
      `<p class="feature">${escapeHtml(job.feature)}</p>` +
      `<p class="branch">${escapeHtml(job.branch)}</p>` +
      `<div class="meta"><span class="round">round ${Number(job.round) || 0}/2</span>` +
      `<span class="age">${formatAge(job.seconds_in_stage)} in stage</span></div>` +
      chips +
      (agents ? `<ul class="agents">${agents}</ul>` : "") +
      reason +
      `</article>`
    );
  }

  function renderBoard(state) {
    const jobs = state.jobs || [];
    const known = new Set(STAGES.map((s) => s.key));
    const unknown = jobs.filter((job) => !known.has(job.stage)).length;
    const columns = STAGES.map((stage) => {
      let items = jobs.filter((job) => job.stage === stage.key);
      const total = items.length;
      if (stage.key === "merged") {
        items = items
          .slice()
          .sort((a, b) => (b.stage_since || 0) - (a.stage_since || 0))
          .slice(0, MERGED_LIMIT);
      }
      const cards = items.length
        ? items.map(renderCard).join("")
        : `<p class="empty">none</p>`;
      return (
        `<section class="column column-${stage.key}">` +
        `<h2>${stage.title}<span class="sub">${stage.sub}</span><span class="count">${total}</span></h2>` +
        cards +
        `</section>`
      );
    });
    const warning = unknown
      ? `<p class="warning">${unknown} job${unknown === 1 ? "" : "s"} with an unknown stage</p>`
      : "";
    return columns.join("") + warning;
  }

  function renderUnassigned(state) {
    const agents = state.unassigned_agents || [];
    if (!agents.length) return "";
    return `<h2>Other agents</h2><ul class="agents">${agents.map(agentLine).join("")}</ul>`;
  }

  function renderFeed(state) {
    const recent = (state.recent || []).slice(0, FEED_LIMIT);
    if (!recent.length) return `<h2>Live feed</h2><p class="empty">No agent activity yet.</p>`;
    const rows = recent.map((event) => {
      const what = [event.tool, event.target].filter(Boolean).join(" ");
      return (
        `<li><time>${clock(event.ts)}</time> ` +
        `<span class="feed-kind">${escapeHtml(event.kind)}</span> ` +
        `<span class="feed-agent">${escapeHtml(shortType(event.agent_type))}</span> ` +
        `<span class="feed-what">${escapeHtml(truncate(what, 70))}</span></li>`
      );
    });
    return `<h2>Live feed</h2><ol class="feed">${rows.join("")}</ol>`;
  }

  function headerStatus(state, error, lastOk) {
    if (error) {
      const since = lastOk ? ` · last update ${clock(lastOk)}` : "";
      return { level: "error", text: `server unreachable${since}` };
    }
    if (state && state.board_error) return { level: "warn", text: state.board_error };
    if (state) return { level: "ok", text: `board.json · ${clock(state.generated_at)}` };
    return { level: "warn", text: "connecting" };
  }

  const api = {
    STAGES,
    escapeHtml,
    formatAge,
    clock,
    reviewChip,
    agentLine,
    renderCard,
    renderBoard,
    renderUnassigned,
    renderFeed,
    headerStatus,
  };

  if (typeof module !== "undefined" && module.exports) {
    module.exports = api;
  } else {
    root.FactoryBoard = api;
  }
})(typeof window !== "undefined" ? window : globalThis);
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test tools/factory-server/tests/`
Expected: all tests pass. If a regex in a test does not match because of whitespace, fix the function output only when the behaviour is wrong, otherwise fix the test.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of `tools/factory-server/static/board.js`, `tools/factory-server/tests/board.test.js`.
Suggested type: `feat(factory): add the dashboard render functions`.

---

### Task 2: Page, styles, polling and server routes

**Files:**
- Create: `tools/factory-server/static/dashboard.css`
- Create: `tools/factory-server/static/app.js`
- Create: `tools/factory-server/tests/seed-demo.sh`
- Modify: `tools/factory-server/static/dashboard.html` (replace the placeholder)
- Modify: `tools/factory-server/src/server.rs` (serve the new assets)

**Interfaces:**
- Consumes: `FactoryBoard` from Task 1, `GET /api/state` from plan 02.
- Produces: the served paths `/`, `/dashboard.html`, `/dashboard.css`, `/board.js`, `/app.js`; and `tests/seed-demo.sh <root>` which builds a populated demo root for a visual check.

- [ ] **Step 1: Write the failing Rust tests for the asset routes**

In `tools/factory-server/src/server.rs`, add inside the existing `mod tests`:

```rust
    #[test]
    fn static_assets_are_served_with_the_right_content_type() -> TestResult {
        let (_dir, app) = app_with_board(None)?;
        let host = Some("127.0.0.1:1");
        let css = route(&app, "GET", "/dashboard.css", host, 1, 5.0);
        assert_eq!(css.status, 200);
        assert_eq!(css.content_type, "text/css; charset=utf-8");
        let board = route(&app, "GET", "/board.js", host, 1, 5.0);
        assert_eq!(board.status, 200);
        assert_eq!(board.content_type, "text/javascript; charset=utf-8");
        let app_js = route(&app, "GET", "/app.js", host, 1, 5.0);
        assert_eq!(app_js.status, 200);
        let page = route(&app, "GET", "/", host, 1, 5.0);
        assert!(String::from_utf8(page.body)?.contains("/board.js"));
        Ok(())
    }
```

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml static_assets`
Expected: FAIL (the routes return 404 and the placeholder page does not mention `/board.js`).

- [ ] **Step 2: Serve the assets**

In `server.rs`, add next to `DASHBOARD_HTML`:

```rust
const DASHBOARD_CSS: &str = include_str!("../static/dashboard.css");
const BOARD_JS: &str = include_str!("../static/board.js");
const APP_JS: &str = include_str!("../static/app.js");
```

Add a helper above `route`:

```rust
fn asset(content_type: &'static str, body: &str) -> Reply {
    Reply {
        status: 200,
        content_type,
        body: body.as_bytes().to_vec(),
    }
}
```

And add three arms to the `match path` in `route`, before `"/healthz"`:

```rust
        "/dashboard.css" => asset("text/css; charset=utf-8", DASHBOARD_CSS),
        "/board.js" => asset("text/javascript; charset=utf-8", BOARD_JS),
        "/app.js" => asset("text/javascript; charset=utf-8", APP_JS),
```

- [ ] **Step 2b: Add security headers (added after the plan 02 review)**

The plan 02 review asked for these headers once the real page exists. First add this test inside the existing `mod tests` in `server.rs` and watch it fail:

```rust
    #[test]
    fn replies_carry_security_headers() -> TestResult {
        let (_dir, app) = app_with_board(Some(JOB))?;
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = server.server_addr().to_ip().ok_or("no ip address")?.port();
        let served = Arc::clone(&app);
        std::thread::spawn(move || serve(server, served, port));
        let reply = http_get(port, "/", &format!("127.0.0.1:{port}"))?.to_lowercase();
        assert!(reply.contains("x-content-type-options: nosniff"), "got: {reply}");
        assert!(reply.contains("frame-ancestors 'none'"), "got: {reply}");
        assert!(reply.contains("default-src 'self'"), "got: {reply}");
        Ok(())
    }
```

Then in `serve`, next to the `Cache-Control` header, add three more headers the same way (each `if let Ok(header) = tiny_http::Header::from_bytes(...) { response.add_header(header); }`):
- `X-Content-Type-Options: nosniff`
- `Content-Security-Policy: default-src 'self'; frame-ancestors 'none'`
- `Referrer-Policy: no-referrer`

The page loads only its own scripts and stylesheet and uses no inline script or inline style, so this policy does not break it. If the page ever needs an inline style, change the page rather than the policy. Run `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` and `cargo fmt --check --manifest-path tools/factory-server/Cargo.toml`.

- [ ] **Step 3: Write the page**

Replace `tools/factory-server/static/dashboard.html`:

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Software factory · Rocket</title>
  <link rel="stylesheet" href="/dashboard.css">
</head>
<body>
  <header class="top">
    <h1>Software factory <span class="project">· Rocket</span></h1>
    <p id="status" class="status status-warn" role="status">
      <span class="dot" aria-hidden="true"></span><span id="status-text">connecting</span>
    </p>
  </header>
  <div class="board-scroll">
    <main id="board" class="board"></main>
  </div>
  <section class="lower">
    <div id="others"></div>
    <div id="feed"></div>
  </section>
  <footer class="commands">
    <code>/factory &lt;feature&gt;</code>
    <code>/factory next</code>
    <code>/factory approve &lt;job-id&gt;</code>
    <code>/factory rework &lt;job-id&gt; &lt;note&gt;</code>
    <code>/factory status</code>
  </footer>
  <script src="/board.js"></script>
  <script src="/app.js"></script>
</body>
</html>
```

- [ ] **Step 4: Write the polling script**

`tools/factory-server/static/app.js`:

```js
"use strict";

(function () {
  const POLL_MS = 1000;
  let lastState = null;
  let lastOk = null;

  function byId(id) {
    return document.getElementById(id);
  }

  function render(state, error) {
    const view = state || lastState;
    if (view) {
      byId("board").innerHTML = FactoryBoard.renderBoard(view);
      byId("others").innerHTML = FactoryBoard.renderUnassigned(view);
      byId("feed").innerHTML = FactoryBoard.renderFeed(view);
    }
    const header = FactoryBoard.headerStatus(state, error, lastOk);
    byId("status").className = `status status-${header.level}`;
    byId("status-text").textContent = header.text;
    byId("board").classList.toggle("is-stale", Boolean(error));
  }

  async function tick() {
    try {
      const response = await fetch("/api/state", { cache: "no-store" });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const state = await response.json();
      lastState = state;
      lastOk = state.generated_at;
      render(state, null);
    } catch (error) {
      render(null, error.message);
    }
  }

  tick();
  setInterval(tick, POLL_MS);
})();
```

- [ ] **Step 5: Write the styles**

`tools/factory-server/static/dashboard.css`:

```css
:root {
  color-scheme: dark;
  --bg: #0e0f13;
  --panel: #161820;
  --border: #2a2d38;
  --text: #e6e7ec;
  --muted: #8b8fa0;
  --ok: #5ec27f;
  --warn: #e0b24a;
  --error: #ef6b6b;
  --accent: #9b8cff;
  --info: #5aa9e6;
  --mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  --sans: system-ui, -apple-system, "Segoe UI", sans-serif;
}

@media (prefers-color-scheme: light) {
  :root {
    color-scheme: light;
    --bg: #f6f6f9;
    --panel: #ffffff;
    --border: #d9dbe3;
    --text: #1b1d26;
    --muted: #5d6275;
    --ok: #1f8f4a;
    --warn: #a36b00;
    --error: #c93a3a;
    --accent: #5b45d6;
    --info: #1f6fb3;
  }
}

* { box-sizing: border-box; }

body {
  margin: 0;
  padding: 1rem;
  background: var(--bg);
  color: var(--text);
  font: 14px/1.45 var(--sans);
}

code, .branch, .job-id, .job-date, time, .agent-status { font-family: var(--mono); }
code { color: var(--muted); font-size: 12px; margin-right: 1rem; white-space: nowrap; }

.top { display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: 0.5rem; }
h1 { font-size: 18px; margin: 0; }
.project { color: var(--muted); font-weight: 400; }

.status { margin: 0; display: flex; align-items: center; gap: 0.5rem; color: var(--muted); font-size: 12px; font-family: var(--mono); }
.status .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--muted); }
.status-ok .dot { background: var(--ok); }
.status-warn .dot { background: var(--warn); }
.status-error .dot { background: var(--error); }
.status-error { color: var(--error); }

.board-scroll { overflow-x: auto; margin-top: 1rem; padding-bottom: 0.5rem; }
.board { display: grid; grid-template-columns: repeat(6, minmax(15rem, 1fr)); gap: 0.75rem; min-width: 96rem; }
.board.is-stale { opacity: 0.55; }

.column h2 { display: flex; align-items: baseline; gap: 0.5rem; font-size: 13px; margin: 0 0 0.5rem; padding-bottom: 0.4rem; border-bottom: 1px solid var(--border); }
.column .sub { color: var(--muted); font-size: 11px; font-weight: 400; font-family: var(--mono); }
.column .count { margin-left: auto; color: var(--muted); font-family: var(--mono); font-size: 12px; }
.column-merged h2 { color: var(--accent); }
.column-needs-human h2 { color: var(--warn); }
.empty { color: var(--muted); margin: 0.25rem 0; }
.warning { color: var(--warn); grid-column: 1 / -1; }

.card { background: var(--panel); border: 1px solid var(--border); border-radius: 8px; padding: 0.7rem 0.8rem; margin-bottom: 0.6rem; overflow-wrap: anywhere; }
.card-needs-human { border-color: var(--warn); }
.card-head { display: flex; gap: 0.5rem; align-items: baseline; font-size: 12px; }
.job-id { color: var(--info); }
.job-slug { color: var(--muted); }
.job-date { margin-left: auto; color: var(--muted); font-size: 11px; }
.feature { margin: 0.4rem 0; }
.branch { margin: 0; color: var(--muted); font-size: 11px; }
.meta { display: flex; justify-content: space-between; margin-top: 0.4rem; color: var(--muted); font-size: 12px; font-family: var(--mono); }

.chips { display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.5rem; }
.chip { border: 1px solid var(--border); border-radius: 999px; padding: 0 0.5rem; font-size: 11px; font-family: var(--mono); color: var(--muted); }
.chip-pass { color: var(--ok); border-color: var(--ok); }
.chip-changes { color: var(--error); border-color: var(--error); }
.chip-running { color: var(--info); border-color: var(--info); }

.agents { list-style: none; margin: 0.5rem 0 0; padding: 0; font-size: 12px; }
.agent { display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.4rem; margin-top: 0.2rem; }
.agent-kind { font-weight: 600; }
.agent-status { color: var(--muted); font-size: 11px; }
.agent-detail { color: var(--muted); }
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--muted); flex: none; }
.dot-running { background: var(--info); animation: pulse 1.6s ease-in-out infinite; }
.dot-stale { background: var(--warn); }
.dot-done { background: var(--ok); }
@keyframes pulse { 50% { opacity: 0.35; } }
@media (prefers-reduced-motion: reduce) { .dot-running { animation: none; } }

.reason { margin: 0.5rem 0 0.25rem; color: var(--warn); }
.hint { margin: 0; }

.lower { display: grid; grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr)); gap: 1rem; margin-top: 1.25rem; }
.lower h2 { font-size: 13px; margin: 0 0 0.4rem; }
.feed { list-style: none; margin: 0; padding: 0; font-size: 12px; }
.feed li { margin-bottom: 0.15rem; overflow-wrap: anywhere; }
.feed time, .feed-kind { color: var(--muted); }

.commands { margin-top: 1.25rem; padding-top: 0.75rem; border-top: 1px solid var(--border); }
```

- [ ] **Step 6: Write the demo seed script**

`tools/factory-server/tests/seed-demo.sh` (make it executable):

```bash
#!/usr/bin/env bash
# Builds a demo factory root with jobs in every stage, for a visual check of the dashboard.
# Usage: seed-demo.sh <empty-dir>
set -eu

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
root="${1:?usage: seed-demo.sh <empty-dir>}"
ctl="$repo/.claude/factory/factory-ctl"
logger="$repo/.claude/factory/hooks/log-event"
log="$root/.superpowers/factory/events.jsonl"

mkdir -p "$root"
git init -q -b main "$root"
git -C "$root" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
export FACTORY_ROOT="$root"
export FACTORY_EVENT_LOG="$log"

run_ctl() { python3 "$ctl" "$@" >/dev/null; }
event() { printf '%s' "$1" | python3 "$logger"; }

python3 "$ctl" new-job "Add a request timeout setting" --date 2026-10-13 >/dev/null
python3 "$ctl" new-job "Fix collection import crash on empty folders" --date 2026-10-13 >/dev/null
python3 "$ctl" new-job "Rename the environment variables panel" --date 2026-10-12 >/dev/null
python3 "$ctl" new-job "Show response size in the status bar" --date 2026-10-12 >/dev/null
python3 "$ctl" new-job "Add a gRPC reflection toggle" --date 2026-10-13 >/dev/null

run_ctl set-stage 1 build --builder-id b-1 --worktree "$root/.claude/worktrees/factory-001"
run_ctl set-stage 2 review --round 1 --review security=pass --review ux=skipped --review ui=skipped --review code=running
run_ctl set-stage 3 needs-human --round 2 --reason "touches the collection trust gate" \
  --review security=changes --review ux=pass --review ui=pass --review code=pass
run_ctl set-stage 4 merged --review security=pass --review ux=pass --review ui=pass --review code=pass
# Job 5 stays in the spec stage.

sleep 1
event '{"hook_event_name":"SubagentStart","agent_id":"b-1","agent_type":"factory-builder","cwd":"'"$root"'/.claude/worktrees/factory-001"}'
event '{"hook_event_name":"PostToolUse","agent_id":"b-1","tool_name":"Edit","tool_input":{"file_path":"src/components/request/TimeoutField.tsx"},"cwd":"'"$root"'/.claude/worktrees/factory-001"}'
event '{"hook_event_name":"PostToolUse","agent_id":"b-1","tool_name":"Bash","tool_input":{"command":"cargo check -j4"},"cwd":"'"$root"'/.claude/worktrees/factory-001"}'
event '{"hook_event_name":"SubagentStart","agent_id":"r-2","agent_type":"factory-code-reviewer","cwd":"'"$root"'"}'
event '{"hook_event_name":"PostToolUse","agent_id":"r-2","tool_name":"Read","tool_input":{"file_path":"crates/rocket-import/src/lib.rs"},"cwd":"'"$root"'"}'
event '{"hook_event_name":"SubagentStart","agent_id":"s-5","agent_type":"factory-spec-writer","cwd":"'"$root"'"}'
event '{"hook_event_name":"PostToolUse","agent_id":"s-5","tool_name":"Grep","tool_input":{"pattern":"reflection"},"cwd":"'"$root"'"}'

echo "Demo root ready: $root"
echo "Start the server:"
echo "  cargo run -j4 --manifest-path $repo/tools/factory-server/Cargo.toml -- --root $root --port 8765"
echo "Then open http://127.0.0.1:8765/"
```


- [ ] **Step 7: Run the tests**

Run:
- `node --test tools/factory-server/tests/`
- `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`

Expected: all Node tests and every Rust test passes (the count is whatever `cargo test` reports).

- [ ] **Step 8: Check it visually**

```bash
bash tools/factory-server/tests/seed-demo.sh /tmp/factory-demo
cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root /tmp/factory-demo --port 8765
```

Open `http://127.0.0.1:8765/`. Expected:
- six columns: Spec holds job 005 with a running `spec-writer`, Build holds job 001 with a running `builder` showing `Bash cargo check -j4`, Review holds job 002 with chips `sec pass`, `ux skipped`, `ui skipped`, `code running` and a `code-reviewer` line, Merged holds job 004, Needs human holds job 003 with its reason and the two commands.
- the header light is green and shows the update time.
- stop the server: within 2 seconds the header turns red with `server unreachable · last update hh:mm:ss` and the board dims but stays.
- resize to about 400px wide: the board scrolls sideways inside its container and the page itself does not.
- switch the OS theme: the page follows it.

Ask the user to look at the page and confirm the look before committing, since the exact visual style is a judgement call.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of `tools/factory-server/static/dashboard.html`, `tools/factory-server/static/dashboard.css`, `tools/factory-server/static/app.js`, `tools/factory-server/tests/seed-demo.sh`, `tools/factory-server/src/server.rs`.
Suggested type: `feat(factory): add the live dashboard page`.

---

### Task 3: End-to-end smoke test and docs

**Files:**
- Create: `tools/factory-server/tests/smoke.sh`
- Create: `tools/factory-server/README.md`
- Modify: `.claude/software-factory.md` (how to run)
- Modify: `CLAUDE.md` (one reference line)
- Modify: `docs/superpowers/plans/2026-10-13-software-factory/00-plan-index.md` (statuses)

**Interfaces:**
- Consumes: `factory-ctl` and `log-event` from plan 01, the server and page from plans 02 and this plan.
- Produces: `bash tools/factory-server/tests/smoke.sh`, which exits non-zero on any failure, and the run instructions.

- [ ] **Step 1: Write the smoke test**

`tools/factory-server/tests/smoke.sh`:

```bash
#!/usr/bin/env bash
# End-to-end check: factory-ctl and the log hook feed the real server, and the API shows it.
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tool="$(cd "$here/.." && pwd)"
repo="$(cd "$tool/../.." && pwd)"
ctl="$repo/.claude/factory/factory-ctl"
logger="$repo/.claude/factory/hooks/log-event"
export PYTHONDONTWRITEBYTECODE=1

tmp="$(mktemp -d "${TMPDIR:-/tmp}/factory-smoke.XXXXXX")"
pid=""
cleanup() {
  if [ -n "$pid" ]; then kill "$pid" 2>/dev/null; fi
  rm -rf "$tmp"
}
trap cleanup EXIT

fails=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1"; fails=$((fails + 1)); }
check() {
  local description="$1"
  shift
  if "$@" >/dev/null 2>&1; then pass "$description"; else fail "$description"; fi
}

cargo build -j4 --manifest-path "$tool/Cargo.toml" -q || { echo "FAIL: build"; exit 1; }
bin="$tool/target/debug/factory-server"

root="$tmp/root"
git init -q -b main "$root"
git -C "$root" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
export FACTORY_ROOT="$root"
export FACTORY_EVENT_LOG="$root/.superpowers/factory/events.jsonl"

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
python3 "$ctl" new-job "Smoke test job" --date 2026-10-13 >/dev/null

"$bin" --root "$root" --port "$port" >"$tmp/server.log" 2>&1 &
pid=$!
for _ in $(seq 1 50); do
  if curl -fs "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

api() { curl -fs "http://127.0.0.1:$port/api/state"; }

check "health endpoint answers" curl -fs "http://127.0.0.1:$port/healthz"
check "new job appears in the spec stage" bash -c "curl -fs http://127.0.0.1:$port/api/state | jq -e '.jobs[0] | select(.id==\"001\" and .stage==\"spec\" and .created==\"2026-10-13\")'"

sleep 1
printf '%s' "{\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"s1\",\"agent_type\":\"factory-spec-writer\",\"cwd\":\"$root\"}" | python3 "$logger"
printf '%s' "{\"hook_event_name\":\"PostToolUse\",\"agent_id\":\"s1\",\"tool_name\":\"Read\",\"tool_input\":{\"file_path\":\"CLAUDE.md\"},\"cwd\":\"$root\"}" | python3 "$logger"
sleep 1.5
check "the spec writer attaches to the job with its live tool" bash -c "curl -fs http://127.0.0.1:$port/api/state | jq -e '.jobs[0].agents[0] | select(.agent_type==\"factory-spec-writer\" and .last_tool==\"Read\" and .status==\"running\")'"

python3 "$ctl" set-stage 1 build --builder-id b1 >/dev/null
check "a stage change shows up on the next poll" bash -c "curl -fs http://127.0.0.1:$port/api/state | jq -e '.jobs[0] | select(.stage==\"build\")'"

printf '%s' "{\"hook_event_name\":\"SubagentStop\",\"agent_id\":\"s1\",\"last_assistant_message\":\"spec written\"}" | python3 "$logger"
sleep 1.5
check "a stopped agent is marked done" bash -c "curl -fs http://127.0.0.1:$port/api/state | jq -e '.unassigned_agents[0] | select(.status==\"done\")'"

check "the page is served" bash -c "curl -fs http://127.0.0.1:$port/ | grep -q 'Software factory'"
check "board.js is served" bash -c "curl -fs http://127.0.0.1:$port/board.js | grep -q FactoryBoard"
code="$(curl -s -o /dev/null -w '%{http_code}' -H 'Host: evil.example' "http://127.0.0.1:$port/api/state")"
if [ "$code" = "403" ]; then pass "a foreign Host header gets 403"; else fail "a foreign Host header gets 403 (got $code)"; fi

echo "garbage line" >>"$FACTORY_EVENT_LOG"
sleep 1
check "a garbage line in the log does not stop the server" curl -fs "http://127.0.0.1:$port/healthz"

if [ "$fails" -eq 0 ]; then echo "all passed"; else echo "$fails failed"; fi
exit $((fails > 0))
```

- [ ] **Step 2: Run it**

Run: `bash tools/factory-server/tests/smoke.sh`
Expected: `all passed`. If the spec-writer line fails, check that the stage rule's `started_ts >= stage_since` holds: the smoke script sleeps 1 second after `new-job` before sending the start event, which is why.

- [ ] **Step 3: Write the README**

`tools/factory-server/README.md`:

````markdown
# factory-server

Local dashboard for Rocket's software factory. It is dev tooling: it is not part of the Rocket app and is not built with it.

## Run

```bash
cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$PWD" --port 8765
```

Open http://127.0.0.1:8765/. `--root` is the main checkout (the folder that holds `.superpowers/`). Default port 8765.

## What it reads

- `.superpowers/factory/board.json`: job stages, written by `.claude/factory/factory-ctl`.
- `.superpowers/factory/events.jsonl`: subagent and tool events, appended by `.claude/factory/hooks/log-event`.

It never writes either file. If it is not running, the factory works the same.

## Endpoints

| Path | Returns |
|---|---|
| `/` and `/dashboard.html` | the page |
| `/dashboard.css`, `/board.js`, `/app.js` | page assets |
| `/api/state` | JSON: `generated_at`, `board_error`, `jobs[]`, `unassigned_agents[]`, `recent[]` |
| `/healthz` | `ok` |

Only `GET` is accepted, only `127.0.0.1` is bound, and a `Host` header other than `127.0.0.1:<port>` or `localhost:<port>` gets 403.

## Tests

```bash
cargo test -j4 --manifest-path tools/factory-server/Cargo.toml
node --test tools/factory-server/tests/
bash tools/factory-server/tests/smoke.sh
```

`tests/seed-demo.sh <dir>` builds a demo root with a job in every stage for a visual check.
````

- [ ] **Step 4: Update the design doc and `CLAUDE.md`**

In `.claude/software-factory.md`, replace the line `Status: design only, nothing built yet. ...` at the top with:

```
Status: built (plans 01 to 03 in `docs/superpowers/plans/2026-10-13-software-factory/`). Run the dashboard with `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$PWD"` and use `/factory <feature>` in an interactive session. Pipeline from the user's chart (2026-10-12), fitted to Rocket's rules on 2026-10-13.
```

In `CLAUDE.md`, add one line after the `See .claude/collection-trust.md ...` line in the rules section:

```
See `.claude/software-factory.md` for the software factory (`/factory` command, agents, live dashboard in `tools/factory-server/`).
```

- [ ] **Step 5: Update the plan index**

In `00-plan-index.md`, set the Status column to `done` for plans 01, 02 and 03, and replace the "not written" file cells of plans 02 and 03 with links to `2026-10-13-software-factory-plan-02-rust-server.md` and `2026-10-13-software-factory-plan-03-dashboard.md`.

- [ ] **Step 6: Run everything once more**

Run:
- `bash .claude/factory/tests/run.sh`
- `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
- `node --test tools/factory-server/tests/`
- `bash tools/factory-server/tests/smoke.sh`
- `yarn tsc --noEmit` and `yarn check` (they must be unaffected)

Expected: all pass.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of `tools/factory-server/tests/smoke.sh`, `tools/factory-server/README.md`, `.claude/software-factory.md`, `CLAUDE.md`, `docs/superpowers/plans/2026-10-13-software-factory/00-plan-index.md`.
Suggested type: `docs(factory): add the smoke test and run instructions`.
