# Factory Failure Details, Plan 03: the dashboard popup

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `needs-human` card with details shows a "See what went wrong" button that opens a popup with the reason, the full evidence and the suggested commands, loaded from `GET /api/jobs/<id>/park`.

**Architecture:** `board.js` gets pure render functions (testable with `node --test`). `app.js` owns one native `dialog`, opens it on a delegated click, fetches the evidence on demand and fills the dialog. The popup lives outside `#board`, so the once-a-second repaint never closes it. Styling reuses the dashboard's existing tokens.

**Tech Stack:** Plain JavaScript (no build step, no framework), CSS, `node:test`, bash.

**Spec:** `docs/superpowers/specs/2026-10-16-factory-failure-details-design.md` (Design 3 and Testing). Plans 01 and 02 are merged on this branch first: jobs in `/api/state` carry `has_park`, `park_kind` and `park_at`, and the endpoint exists.

## Global Constraints

- The dashboard follows its own design system, see the comment at the top of `static/dashboard.css`: one monospace face, cool-grey oklch palette, hairline borders instead of shadows, 10px panels, pill badges, green and red only for pass and fail. Add no fonts, no new colors (use the existing tokens `--destructive`, `--destructive-wash`, `--attention`, `--attention-wash`, `--primary`, `--muted`, `--border`, `--card`), no gradients, no shadows, no animation. The one distinctive element is the marker on the lines that matter.
- The CSP is `default-src 'self'`: no inline `style` attributes and no inline scripts. All styling is in `dashboard.css`.
- Everything that goes into HTML text or attributes passes `escapeHtml`. Evidence text is untrusted.
- The page must work with the keyboard (Tab order, Esc closes, visible focus ring), at 400 px wide with a side gutter of at least 16 px, and in light and dark themes (tokens only).
- The popup is a native `dialog` opened with `showModal()`. It lives outside `#board`.
- Writing copy: sentence case, plain words, active voice, errors say what happened and what to do. No emojis.
- Commands: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js` for JavaScript, `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` for the server (never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`).
- Commit steps are for a human running the plan by hand: pathspec commit only, message through the skill `dev-workflow-skills:1-git-commit`, never `git add -A` or `git add .`.

## Review Focus

1. Evidence text containing `<script>`, quotes or `&` must show as text and never run. Pinned in Task 1.
2. The popup must survive the board repaint: the board refreshes every second and replaces card nodes, but the dialog and its content must stay as they are. Pinned in Task 2 by keeping the dialog outside `#board` and a test that the card markup carries no dialog.
3. A 404, a server error and a network error each show a clear message, and an error offers "Try again". Pinned in Tasks 1 and 2.
4. A copied command must give feedback while the modal dialog is open (the toast would otherwise sit behind the dialog). Pinned in Task 2.
5. Empty lines in the evidence must keep their height, and a text with thousands of lines must still render. Pinned in Task 1.

## File Structure

```
tools/factory-server/static/board.js        park render functions and the card button
tools/factory-server/static/app.js          dialog controller
tools/factory-server/static/dashboard.html  the dialog element
tools/factory-server/static/dashboard.css   popup styles
tools/factory-server/tests/board.test.js    render tests
tools/factory-server/tests/seed-demo.sh     demo park.json for job 003
tools/factory-server/tests/smoke.sh         endpoint checks
tools/factory-server/README.md              test notes
.claude/software-factory.md                 documentation
docs/superpowers/specs/2026-10-16-factory-failure-details-design.md  marker rule
```

---

### Task 1: Render functions and the card button

**Files:**
- Modify: `tools/factory-server/static/board.js`
- Modify: `tools/factory-server/tests/board.test.js`
- Modify: `docs/superpowers/specs/2026-10-16-factory-failure-details-design.md`

**Interfaces:**
- Consumes: existing `escapeHtml`, `formatAge`, `copyChip`, `renderCard(job, now, specs)`.
- Produces (all exported on the `FactoryBoard` API object):
  - `parkKindLabel(kind) -> string` and `parkKindClass(kind) -> "chip-changes" | "chip-warn"`.
  - `markLines(text) -> string` (HTML, one `<span class="ln">` per line, hits get `ln ln-hit`).
  - `renderParkDialog(job, view, now) -> string` (HTML for the dialog content).
  - `parkMessage(job, text, retry) -> string` (HTML for loading, empty and error states).
  - `renderCard` shows `<button class="cmd park-open" data-park="<id>">See what went wrong</button>` for a `needs-human` job whose `has_park` is true.

- [ ] **Step 1: Write the failing tests**

Append to `tools/factory-server/tests/board.test.js`:

```js
test("a needs-human card with details has a button, one without has none", () => {
  const withPark = board.renderCard(makeJob({ stage: "needs-human", reason: "why", has_park: true }), 200, []);
  assert.match(withPark, /<button type="button" class="cmd park-open" data-park="001">See what went wrong<\/button>/);
  const without = board.renderCard(makeJob({ stage: "needs-human", reason: "why" }), 200, []);
  assert.doesNotMatch(without, /data-park=/);
  const building = board.renderCard(makeJob({ stage: "build", has_park: true }), 200, []);
  assert.doesNotMatch(building, /data-park=/);
});

test("the card never contains the popup, so a repaint cannot close it", () => {
  const html = board.renderCard(makeJob({ stage: "needs-human", reason: "why", has_park: true }), 200, []);
  assert.doesNotMatch(html, /<dialog|park-text/);
});

test("parkKindLabel and parkKindClass cover every kind", () => {
  const red = ["checks-failed", "changes-remain", "builder-stopped", "commit-failed"];
  const amber = ["risky-diff", "diff-too-large", "budget-stopped", "builder-committed", "nothing-changed", "other"];
  for (const kind of red) assert.equal(board.parkKindClass(kind), "chip-changes", kind);
  for (const kind of amber) assert.equal(board.parkKindClass(kind), "chip-warn", kind);
  assert.equal(board.parkKindClass("something-new"), "chip-warn");
  assert.equal(board.parkKindLabel("checks-failed"), "Checks failed");
  assert.equal(board.parkKindLabel("other"), "Needs a human");
  assert.equal(board.parkKindLabel("something-new"), "Needs a human");
});

test("markLines marks the lines that matter and escapes the text", () => {
  const html = board.markLines(
    "error[E0432]: unresolved import\n  --> src/a.rs\ntest a::b ... FAILED\nthread 'x' panicked at src/c.rs:1\n[blocker] src/d.rs wrong\nFAIL cargo check\nall fine",
  );
  const hits = html.match(/class="ln ln-hit"/g) || [];
  assert.equal(hits.length, 5);
  assert.match(html, /<span class="ln">all fine<\/span>/);
  assert.match(html, /<span class="ln">  --&gt; src\/a.rs<\/span>/);
});

test("markLines escapes markup and keeps empty lines at full height", () => {
  const html = board.markLines('<script>alert("x")</script>\n\nlast\n');
  assert.doesNotMatch(html, /<script>/);
  assert.match(html, /&lt;script&gt;alert\(&quot;x&quot;\)&lt;\/script&gt;/);
  assert.match(html, /<span class="ln"> <\/span>/);
  // The trailing newline does not add a phantom last line.
  assert.equal((html.match(/class="ln/g) || []).length, 3);
});

test("markLines renders thousands of lines", () => {
  const text = Array.from({ length: 5000 }, (_, i) => `line ${i}`).join("\n");
  assert.equal((board.markLines(text).match(/class="ln/g) || []).length, 5000);
});

test("renderParkDialog shows the kind, reason, parts and next commands", () => {
  const job = makeJob({ stage: "needs-human", reason: "fallback" });
  const view = {
    kind: "checks-failed",
    reason: "checks failed after 2 fix rounds",
    at: 100,
    round: 3,
    next: ["factory rework 001 <note>"],
    parts: [
      { title: "Failing checks", source: "checks.md", text: "FAIL cargo check\nerror: boom", truncated: false },
      { title: "Cut log", source: "log", text: "tail", truncated: true },
    ],
  };
  const html = board.renderParkDialog(job, view, 220);
  assert.match(html, /<h2 id="park-title" class="park-title">checks failed after 2 fix rounds<\/h2>/);
  assert.match(html, /class="chip chip-changes">Checks failed</);
  assert.match(html, /parked 2m ago/);
  assert.match(html, /Failing checks/);
  assert.match(html, /checks\.md/);
  assert.match(html, /class="ln ln-hit">FAIL cargo check/);
  assert.match(html, /The start of this text was cut/);
  assert.match(html, /What to do next/);
  assert.match(html, /data-copy="factory rework 001 &lt;note&gt;"/);
  assert.match(html, /data-park-close/);
  assert.match(html, /class="park-note" role="status" hidden/);
});

test("renderParkDialog escapes the reason and survives missing fields", () => {
  const html = board.renderParkDialog(makeJob({ reason: null }), { reason: "<img onerror=x>" }, 10);
  assert.doesNotMatch(html, /<img/);
  assert.match(html, /&lt;img onerror=x&gt;/);
  assert.match(html, /No evidence was recorded for this park/);
  assert.doesNotMatch(html, /What to do next/);
  const bare = board.renderParkDialog(makeJob({ reason: null }), {}, 10);
  assert.match(bare, /Needs a human/);
});

test("parkMessage offers a retry only when asked", () => {
  const job = makeJob({});
  assert.match(board.parkMessage(job, "Loading.", false), /Loading\./);
  assert.doesNotMatch(board.parkMessage(job, "Loading.", false), /data-park-retry/);
  assert.match(board.parkMessage(job, "It failed.", true), /data-park-retry/);
  assert.match(board.parkMessage(job, "<b>", false), /&lt;b&gt;/);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: FAIL, `board.parkKindClass is not a function` and the like.

- [ ] **Step 3: Implement**

In `board.js`, above `function renderCard`, add:

```js
  const PARK_LABEL = {
    "checks-failed": "Checks failed",
    "changes-remain": "Changes remain",
    "risky-diff": "Risky diff",
    "diff-too-large": "Diff too large",
    "budget-stopped": "Budget stopped",
    "builder-stopped": "Builder stopped",
    "builder-committed": "Builder committed",
    "nothing-changed": "Nothing changed",
    "commit-failed": "Commit failed",
    other: "Needs a human",
  };
  const PARK_RED = ["checks-failed", "changes-remain", "builder-stopped", "commit-failed"];
  // A line that starts with an error word, ends a failed test, or reports a panic is a line that matters.
  const PARK_HIT = /^\s*(error|FAIL|\[blocker\])|\sFAILED\s*$|panicked at/;

  function parkKindLabel(kind) {
    return PARK_LABEL[kind] || PARK_LABEL.other;
  }

  function parkKindClass(kind) {
    return PARK_RED.includes(kind) ? "chip-changes" : "chip-warn";
  }

  // One block-level span per line. Empty lines hold a space so they keep their height.
  function markLines(text) {
    const lines = String(text === null || text === undefined ? "" : text).split("\n");
    if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
    return lines
      .map((line) => `<span class="ln${PARK_HIT.test(line) ? " ln-hit" : ""}">${line === "" ? " " : escapeHtml(line)}</span>`)
      .join("");
  }

  function parkHeader(job, kindHtml, whenHtml, title) {
    return (
      `<header class="park-head"><div class="park-meta"><span class="job-id">${escapeHtml(job.id)}</span>${kindHtml}${whenHtml}</div>` +
      `<h2 id="park-title" class="park-title">${escapeHtml(title)}</h2>` +
      `<button type="button" class="cmd park-close" data-park-close>Close</button></header>`
    );
  }

  function renderParkPart(part) {
    const cut = part.truncated
      ? `<p class="park-cut">The start of this text was cut. Only the last part is shown.</p>`
      : "";
    return (
      `<section class="park-part"><h3 class="park-part-title">${escapeHtml(part.title)} <span class="park-source">${escapeHtml(part.source)}</span></h3>` +
      cut +
      `<pre class="park-text" tabindex="0" aria-label="${escapeHtml(part.title)}">${markLines(part.text)}</pre></section>`
    );
  }

  function renderParkDialog(job, view, now) {
    const kind = String(view.kind || "other");
    const kindHtml = `<span class="chip ${parkKindClass(kind)}">${escapeHtml(parkKindLabel(kind))}</span>`;
    const whenHtml =
      Number(view.at) > 0
        ? `<time class="park-when">parked ${escapeHtml(formatAge(Math.max(0, now - Number(view.at))))} ago</time>`
        : "";
    const parts = (view.parts || []).map(renderParkPart).join("");
    const next = (view.next || []).map(copyChip).join("");
    return (
      parkHeader(job, kindHtml, whenHtml, view.reason || job.reason || "Needs a human") +
      `<p class="park-note" role="status" hidden>This job is no longer waiting for you. The details below are from when it parked.</p>` +
      `<div class="park-body">${parts || `<p class="park-empty">No evidence was recorded for this park.</p>`}</div>` +
      (next ? `<footer class="park-foot"><p class="park-next-label">What to do next</p><p class="hint">${next}</p></footer>` : "")
    );
  }

  // Loading, empty and error states of the popup.
  function parkMessage(job, text, retry) {
    return (
      parkHeader(job, "", "", "What went wrong") +
      `<div class="park-body"><p class="park-empty">${escapeHtml(text)}</p>` +
      (retry ? `<p><button type="button" class="cmd" data-park-retry>Try again</button></p>` : "") +
      `</div>`
    );
  }

  function parkButton(job) {
    if (job.stage !== "needs-human" || !job.has_park) return "";
    return `<p class="park-link"><button type="button" class="cmd park-open" data-park="${escapeHtml(job.id)}">See what went wrong</button></p>`;
  }
```

In `renderCard`, change the `reason` constant so the button sits between the reason and the hint:

```js
    const reason =
      job.stage === "needs-human"
        ? `<p class="reason">${escapeHtml(job.reason || "needs a human")}</p>` +
          parkButton(job) +
          `<p class="hint">${copyChip(`factory approve ${job.id}`)}${copyChip(`factory rework ${job.id} <note>`)}${copyChip(`factory cancel ${job.id}`)}</p>`
        : "";
```

and add `parkKindLabel, parkKindClass, markLines, renderParkDialog, parkMessage, parkButton,` to the exported `api` object (next to `copyChip`).

In the spec file, replace the Marker bullet in Design 3 ("Lines that start with `error`, `FAIL`, `FAILED` or `[blocker]` get a red rule...") and the matching acceptance criterion with: "Lines that start with `error`, `FAIL` or `[blocker]` (after leading spaces), lines that end with `FAILED`, and lines that contain `panicked at` get a red rule at the left edge, and the first one is scrolled into view on open."

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`
Expected: all pass, including every earlier test (the needs-human card tests must still pass).

- [ ] **Step 5: Commit**

Commit `board.js`, `board.test.js` and the spec file with the skill message `feat(factory-server): render park details for the popup`.

---

### Task 2: The dialog, its controller and its styles

**Files:**
- Modify: `tools/factory-server/static/dashboard.html`
- Modify: `tools/factory-server/static/app.js`
- Modify: `tools/factory-server/static/dashboard.css`

**Interfaces:**
- Consumes: Task 1 render functions on `FactoryBoard`, `GET /api/jobs/<id>/park` from plan 02, `lastState` in `app.js`.
- Produces: the dialog element `#park-dialog` with the content holder `#park-content`.

There is no browser test harness here, so this task is checked by the Rust embed tests, `node --check` on `app.js`, a served-asset check, and the manual look in Task 3.

- [ ] **Step 1: Write the failing check**

Add to the tests module of `tools/factory-server/src/server.rs`:

```rust
    #[test]
    fn the_page_carries_the_park_dialog_and_its_script_and_styles() -> TestResult {
        let (_dir, app) = app_with_board(Some(JOB))?;
        let host = Some("127.0.0.1:1");
        let page = route(&app, "GET", "/", host, 1, 5.0);
        let html = String::from_utf8(page.body)?;
        assert!(html.contains(r#"<dialog id="park-dialog""#), "dialog element");
        assert!(html.contains(r#"id="park-content""#), "content holder");
        let app_js = String::from_utf8(route(&app, "GET", "/app.js", host, 1, 5.0).body)?;
        assert!(app_js.contains("/park") && app_js.contains("showModal"), "controller");
        let css = String::from_utf8(route(&app, "GET", "/dashboard.css", host, 1, 5.0).body)?;
        assert!(css.contains(".park-dialog") && css.contains(".ln-hit"), "styles");
        Ok(())
    }
```

- [ ] **Step 2: Run the check to verify it fails**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml the_page_carries_the_park_dialog`
Expected: FAIL on the first assert (no dialog element yet).

- [ ] **Step 3: Implement**

`dashboard.html`: add the dialog right before the script tags (outside `.page`'s board, still inside `.page` is fine, but keep it a sibling of `#board`'s ancestors so a repaint never touches it):

```html
  <dialog id="park-dialog" class="park-dialog" aria-labelledby="park-title">
    <div id="park-content"></div>
  </dialog>
```

`app.js`: inside the IIFE, add below `showToast`'s neighbours. Replace `showToast` and the click listener, and add the controller:

```js
  const dialog = byId("park-dialog");
  const parkContent = byId("park-content");
  let parkJobId = null;
  let parkOpener = null;

  // A modal dialog sits above everything, so the toast moves into it while it is open.
  function showToast(message) {
    const toast = byId("toast");
    const host = dialog.open ? dialog : document.querySelector(".page");
    if (toast.parentElement !== host) host.appendChild(toast);
    toast.textContent = message;
    toast.hidden = false;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast.hidden = true;
      toast.textContent = "";
    }, 2000);
  }

  function findJob(id) {
    return lastState && lastState.jobs ? lastState.jobs.find((job) => job.id === id) : null;
  }

  function nowSeconds() {
    return lastState ? lastState.generated_at : Date.now() / 1000;
  }

  // Tells the user when the job moved on while the popup was open.
  function syncParkNote() {
    if (!dialog.open || !parkJobId) return;
    const job = findJob(parkJobId);
    const note = parkContent.querySelector(".park-note");
    if (note) note.hidden = !job || job.stage === "needs-human";
  }

  async function loadPark(id) {
    const job = findJob(id) || { id, reason: null };
    try {
      const response = await fetch(`/api/jobs/${encodeURIComponent(id)}/park`, { cache: "no-store" });
      if (parkJobId !== id) return;
      if (response.status === 404) {
        parkContent.innerHTML = FactoryBoard.parkMessage(job, "No details were recorded for this job.", false);
        return;
      }
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const view = await response.json();
      if (parkJobId !== id) return;
      parkContent.innerHTML = FactoryBoard.renderParkDialog(job, view, nowSeconds());
      syncParkNote();
      const hit = parkContent.querySelector(".ln-hit");
      if (hit) hit.scrollIntoView({ block: "center" });
    } catch (error) {
      if (parkJobId !== id) return;
      parkContent.innerHTML = FactoryBoard.parkMessage(
        job,
        `The details could not be loaded (${error.message}). Check that the server is running, then try again.`,
        true,
      );
    }
  }

  function openPark(id, opener) {
    const job = findJob(id) || { id, reason: null };
    parkJobId = id;
    parkOpener = opener || null;
    parkContent.innerHTML = FactoryBoard.parkMessage(job, "Loading the details.", false);
    if (!dialog.open) dialog.showModal();
    loadPark(id);
  }

  dialog.addEventListener("close", () => {
    const id = parkJobId;
    parkJobId = null;
    const toast = byId("toast");
    if (toast.parentElement !== document.querySelector(".page")) document.querySelector(".page").appendChild(toast);
    toast.hidden = true;
    // The card is redrawn every second, so find the button again when the old node is gone.
    const opener = parkOpener && document.contains(parkOpener) ? parkOpener : document.querySelector(`[data-park="${id}"]`);
    if (opener) opener.focus();
    parkOpener = null;
  });

  document.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (!target) return;
    const copy = target.closest("[data-copy]");
    if (copy) {
      copyText(copy.getAttribute("data-copy") || "");
      return;
    }
    const open = target.closest("[data-park]");
    if (open) {
      openPark(open.getAttribute("data-park") || "", open);
      return;
    }
    if (target.closest("[data-park-close]")) {
      dialog.close();
      return;
    }
    if (target.closest("[data-park-retry]")) {
      if (parkJobId) openPark(parkJobId, parkOpener);
      return;
    }
    // A click on the backdrop lands on the dialog element itself.
    if (target === dialog) dialog.close();
  });
```

Remove the old `showToast` function and the old `document.addEventListener("click", ...)` block (the new code replaces both). In `render()`, after `applyBarWidths();` add `syncParkNote();`.

`dashboard.css`: append (all values from the existing tokens):

```css
/* The failure details popup. */
.park-link { margin: 0 0 0.45rem; }
.chip-warn { border-color: var(--attention); background: var(--attention-wash); color: var(--attention); }
.park-dialog { width: min(52rem, calc(100% - 2rem)); max-height: calc(100% - 2rem); margin: auto; padding: 0; border: 1px solid var(--border); border-radius: var(--radius); background: var(--card); color: var(--foreground); overflow: hidden; }
.park-dialog::backdrop { background: oklch(0.2 0.01 230 / 45%); }
.park-dialog[open] { display: flex; flex-direction: column; }
#park-content { display: flex; flex-direction: column; min-height: 0; }
.park-head { display: grid; grid-template-columns: 1fr auto; gap: 0.4rem 0.75rem; align-items: start; padding: 0.875rem 0.875rem 0.7rem; border-bottom: 1px solid var(--border); }
.park-meta { grid-column: 1 / -1; display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; color: var(--muted-foreground); font-size: 11.5px; }
.park-title { margin: 0; font-size: 14px; font-weight: 600; overflow-wrap: anywhere; }
.park-close { align-self: start; }
.park-note { margin: 0; padding: 0.5rem 0.875rem; border-bottom: 1px solid var(--border); background: var(--attention-wash); color: var(--attention); font-size: 12px; }
.park-body { min-height: 0; padding: 0.875rem; overflow: auto; }
.park-part + .park-part { margin-top: 1rem; }
.park-part-title { margin: 0 0 0.4rem; font-size: 12px; font-weight: 600; }
.park-source { margin-left: 0.35rem; color: var(--muted-foreground); font-weight: 400; }
.park-cut { margin: 0 0 0.4rem; color: var(--attention); font-size: 12px; }
.park-text { margin: 0; max-height: 22rem; padding: 0.5rem 0; border: 1px solid var(--border); border-radius: 6px; background: var(--muted); overflow: auto; font: 12px/1.55 var(--font); }
.park-text:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.ln { display: block; padding: 0 0.75rem 0 0.6rem; border-left: 2px solid transparent; white-space: pre; }
.ln-hit { border-left-color: var(--destructive); background: var(--destructive-wash); }
.park-empty { margin: 0; color: var(--muted-foreground); }
.park-foot { padding: 0.7rem 0.875rem 0.875rem; border-top: 1px solid var(--border); }
.park-next-label { margin: 0 0 0.4rem; color: var(--muted-foreground); font-size: 12px; }
```

(The `.park-note` rule must not set `display`, so the `hidden` attribute keeps hiding it.) The `.toast` rule already exists and works inside the dialog because it is `position: fixed`.

- [ ] **Step 4: Run the checks to verify they pass**

Run: `node --check tools/factory-server/static/app.js`, then `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` and `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`.
Expected: all pass.

- [ ] **Step 5: Commit**

Commit `dashboard.html`, `app.js`, `dashboard.css` and `server.rs` with the skill message `feat(factory-server): add the failure details popup`.

---

### Task 3: Demo data, smoke checks and documentation

**Files:**
- Modify: `tools/factory-server/tests/seed-demo.sh`
- Modify: `tools/factory-server/tests/smoke.sh`
- Modify: `tools/factory-server/README.md`
- Modify: `.claude/software-factory.md`

**Interfaces:**
- Consumes: everything above.
- Produces: a demo root whose job 003 is parked with details, a smoke check of the endpoint, and the docs.

- [ ] **Step 1: Write the failing smoke check**

In `smoke.sh`, add a `write_board_parked` helper next to `write_board`, and the checks after the `a stage change shows up on the next poll` line (before the merged board is written):

```bash
# A parked job whose folder holds a park.json.
write_board_parked() {
  local now
  now="$(date +%s)"
  cat >"$board.tmp" <<JSON
{"jobs":[{"id":"001","dir":"2026-10-13-001-smoke","slug":"smoke","created":"2026-10-13","feature":"Smoke test job","branch":"factory/001-smoke","stage":"needs-human","reason":"checks failed after 2 fix rounds","park_kind":"checks-failed","park_at":$now,"round":3,"stage_since":$now,"reviews":{"code":"pending"}}]}
JSON
  mv "$board.tmp" "$board"
}
```

```bash
write_board_parked
await "a parked job without park.json has no details yet" '.jobs[0] | select(.stage=="needs-human" and .has_park==false)'
check "the park endpoint answers 404 without a file" bash -c "[ \"\$(curl -s -o /dev/null -w '%{http_code}' $url/api/jobs/001/park)\" = 404 ]"
mkdir -p "$root/.superpowers/sdd/2026-10-13-001-smoke"
cat >"$root/.superpowers/sdd/2026-10-13-001-smoke/park.json" <<JSON
{"kind":"checks-failed","reason":"checks failed after 2 fix rounds","at":$now,"round":3,"next":["factory rework 001 <note>"],"parts":[{"title":"Failing checks","source":"checks.md","text":"FAIL cargo check\nerror: boom","truncated":false}]}
JSON
await "a parked job with park.json shows has_park" '.jobs[0] | select(.has_park==true and .park_kind=="checks-failed")'
check "the park endpoint serves the evidence" bash -c "curl -fs $url/api/jobs/001/park | grep -q 'error: boom'"
check "the polled state never carries the evidence text" bash -c "! curl -fs $url/api/state | grep -q 'error: boom'"
check "the park endpoint refuses a foreign Host" bash -c "[ \"\$(curl -s -o /dev/null -w '%{http_code}' -H 'Host: evil.example' $url/api/jobs/001/park)\" = 403 ]"
```

Run: `bash tools/factory-server/tests/smoke.sh`
Expected before Task 3's seed and docs changes: the new smoke lines PASS (the server work is already done), so this step is the verification, not a red step. If any of them fails, stop and fix the server code of plan 02.

- [ ] **Step 2: Seed the demo**

In `seed-demo.sh`, after the `mv "$data/board.json.tmp" "$data/board.json"` line, add:

```bash
# Job 003 is parked with details, for a look at the popup.
sed -e "s/{\"id\":\"003\",/{\"id\":\"003\",\"park_kind\":\"checks-failed\",\"park_at\":$((now - 120)),/" "$data/board.json" >"$data/board.json.tmp"
mv "$data/board.json.tmp" "$data/board.json"
mkdir -p "$sdd/2026-10-13-003-demo"
cat >"$sdd/2026-10-13-003-demo/park.json" <<JSON
{"kind":"checks-failed","reason":"checks failed after 2 fix rounds","at":$((now - 120)),"round":3,
 "next":["factory rework 003 <note>"],
 "parts":[{"title":"Failing checks","source":"checks.md","truncated":false,
  "text":"## cargo test -j4 -p factory-engine (exit 101)\nrunning 3 tests\ntest ports::tests::usage_line_has_the_counters ... ok\ntest ports::tests::a_killed_run_names_its_stop_reason ... FAILED\n\nfailures:\n\n---- ports::tests::a_killed_run_names_its_stop_reason stdout ----\nthread 'ports::tests::a_killed_run_names_its_stop_reason' panicked at src/ports.rs:171:9:\nassertion \`left == right\` failed\n  left: \"done\"\n right: \"budget-stop\"\n\nerror: test failed, to rerun pass \`-p factory-engine --lib\`"}]}
JSON
```

- [ ] **Step 3: Documentation**

In `tools/factory-server/README.md`, in the test section, add one line after the smoke command: `The smoke test also covers the park endpoint (404 without a file, the evidence with one, no evidence text in /api/state).` and one line after the seed-demo line: `Job 003 in the demo is parked with details, so its card shows the "See what went wrong" button.`

In `.claude/software-factory.md` add, in the Job model section after the `board.json` bullet, a bullet:

`- `park.json` (job folder, written by the driver when it parks a job): `kind`, `reason`, `at`, `round`, `next` (the commands to run) and `parts` (title, source, text, truncated; each text capped at 200 KB, keeping the end). Kinds: `checks-failed`, `changes-remain`, `risky-diff`, `diff-too-large`, `budget-stopped`, `builder-stopped`, `builder-committed`, `nothing-changed`, `commit-failed`, `other`. The board job has `park_kind` and `park_at`, cleared when the job leaves needs-human. `factory status` prints the reason, the first 12 lines of the first part and the next commands for each parked job.`

and, in the Dashboard section, a bullet:

`- A needs-human card with details (`has_park` in `/api/state`) shows a "See what went wrong" button. It opens a native dialog that fetches `GET /api/jobs/<id>/park` (404 without a file, 422 for a broken file, text capped at 1 MiB, the path taken from the board and kept inside `.superpowers/sdd/`). The popup shows the kind, reason, each evidence part with the lines that matter marked and the first one scrolled into view, and the next commands as copy chips. The polled state never carries the evidence text.`

- [ ] **Step 4: Verify**

Run, one per call: `bash tools/factory-server/tests/smoke.sh` (expect `all passed`), `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`, `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`, and `bash tools/factory-server/tests/seed-demo.sh /tmp/claude-1000/factory-demo` (expect `Demo root ready`; then `grep -c '"park_kind":"checks-failed"' /tmp/claude-1000/factory-demo/.superpowers/factory/board.json` prints 1).

Manual look (left to the user, agents cannot see a browser): start the server with the command the seed script prints, open the page in a real window, open job 003's popup in the light and the dark theme and at phone width, check Tab order, Esc, the red marker on the failing lines, the scroll to the first one, the copy chip with its toast, and the "Try again" path by stopping the server while the popup is open.

- [ ] **Step 5: Commit**

Commit `seed-demo.sh`, `smoke.sh`, `README.md` and `.claude/software-factory.md` with the skill message `docs(factory): document park details and extend the smoke test`.

Next plan: none
