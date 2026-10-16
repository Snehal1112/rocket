# Factory failure details (sub-project B)

Part of a three-part factory improvement: A accurate usage (built), B failure details on `needs-human` cards, C cost report and estimate by job size. B is independent of A's code. It continues on the branch `factory-accurate-usage` and merges with A.

## Goal

When a factory job parks in `needs-human`, the driver records why and keeps the evidence, and the dashboard and `factory status` show the reason, the evidence and the next step to take.

## Background

- The engine parks a job at about 11 places (`build.rs`, `review.rs`, `checks_stage.rs`, `decide.rs`). Each passes one short string, stored as `reason` on the board.
- The evidence already exists on disk but is not shown: failing checks in `checks.md`, reviewer findings in `review-code-rN.md` and `changes-rN.md`, the matched risk paths in the risky-diff summary, the builder's last message.
- The dashboard card prints only the reason string (`tools/factory-server/static/board.js`, `renderCard` and the summary row). `factory status` prints the same string.
- The driver is Rust and the dashboard server only reads files, so none of this costs LLM tokens. The cost is payload size, because a failing test log can be large.

## Decisions

- The driver writes the evidence once, at park time, to `park.json` in the job folder. The dashboard server never writes it.
- The board keeps only two new optional fields, `park_kind` and `park_at`. The board JSON stays compatible with `factory-server`.
- The full evidence is not in the polled `/api/state`. A new read-only endpoint serves it when the popup opens.
- Each evidence part is capped at 200 KB when written. The cap keeps the end of the text and marks the part `truncated`. The endpoint reply is capped at 1 MiB.
- Writing `park.json` is best effort: a write error never stops the park.
- Jobs parked before this change have no `park.json`. They show the reason only, and the popup button is hidden.
- The popup uses the dashboard's existing look (one monospace face, cool-grey oklch palette, hairline borders, 10px radius, teal for identity, red and amber washes for status). No new fonts or colors. The one distinctive element is a marker on the lines that matter.
- The popup is a native `dialog` element opened with `showModal`. The board is re-rendered by polling, so the popup lives outside the board root.
- No actions are added to the dashboard. It stays read-only, and next steps are copy chips of `factory` commands.

## Design

### 1. park.json (driver)

`park()` in `factory-engine/src/ports.rs` gains a typed input instead of a bare string, and writes `<job dir>/park.json` just before the board moves to `needs-human`.

```json
{
  "kind": "checks-failed",
  "reason": "checks failed after 2 fix rounds",
  "at": 1760000000,
  "round": 3,
  "next": ["factory rework 012 <note>"],
  "parts": [
    { "title": "Failing checks", "source": "checks.md", "text": "...", "truncated": false }
  ]
}
```

`kind` is one of: `checks-failed`, `changes-remain`, `risky-diff`, `diff-too-large`, `budget-stopped`, `builder-stopped`, `builder-committed`, `nothing-changed`, `commit-failed`, `other` (any other park: merge conflicts, main moved, a run loop that did not settle).

| Park site | kind | parts | next |
|---|---|---|---|
| `decide.rs` checks failed after fix rounds | `checks-failed` | failing check names and output from `checks.md` | `factory rework <id> <note>` |
| `decide.rs` changes remain after fix rounds | `changes-remain` | the reviewer's `[blocker]` lines from `review-code-rN.md` | `factory rework <id> <note>`, `factory approve <id>` |
| `decide.rs` risky diff | `risky-diff` | the risk summary, then the diff stat | `factory approve <id>`, `factory rework <id> <note>` |
| `decide.rs` and `build.rs` diff over the cap | `diff-too-large` | changed lines against the cap | `factory approve <id>`, `factory rework <id> <note>` |
| `decide.rs` budget stopped | `budget-stopped` | weighted tokens against the budget | `factory approve <id>`, `factory cancel <id>` |
| `build.rs` builder stopped | `builder-stopped` | stop reason and the builder's last message | `factory rework <id> <note>` |
| `build.rs` builder committed on its own | `builder-committed` | the park reason, which holds the recovery sha | `factory rework <id> <note>` |
| `build.rs` nothing changed | `nothing-changed` | the park reason | `factory rework <id> <note>`, `factory cancel <id>` |
| commit failures from `commit.rs` | `commit-failed` | the park reason | `factory rework <id> <note>` |
| any other park | `other` | none | `factory approve <id>`, `factory rework <id> <note>`, `factory cancel <id>` |

A part that cannot be read (file missing) is left out, and the reason stays as the first line. `next` holds the commands with the real job id filled in.

Board: `Job` gains `park_kind: Option<String>` and `park_at: Option<u64>`, both skipped when empty.

`factory status` prints, for a parked job, the reason, the first part's first 12 lines, and the `next` lines.

### 2. Endpoint (factory-server)

`GET /api/jobs/<id>/park` is routed in `server.rs` `route()`.

- The id must match the job-id pattern and exist on the board. The file path comes from the board's job folder, never from the URL, and must resolve inside the project's `.superpowers/sdd/` folder.
- The server parses `park.json`, keeps only the known fields, and replies with `Cache-Control: no-store`. A reply over 1 MiB is cut with `truncated: true`.
- Missing file: 404. Unparsable file: 422. Non-GET: 405. The existing host check applies.
- `/api/state` adds `has_park: bool` per job so the card knows whether to show the button.

### 3. Popup (dashboard)

- Card: the reason line and copy chips stay. A button, "See what went wrong", is added when `has_park` is true.
- The popup is a `dialog`: a header with the job id, a kind pill (red wash for `checks-failed`, `changes-remain`, `builder-stopped`, `commit-failed`; amber wash for `risky-diff`, `diff-too-large`, `budget-stopped`, `builder-committed`, `nothing-changed`), the reason as the title and when it parked; a body with one section per part (title, source file in muted text, a bordered scrollable text block with no wrapping and a note when truncated); a footer "What to do next" with the `next` commands as copy chips.
- Marker: lines that start with `error`, `FAIL`, `FAILED` or `[blocker]` get a red rule at the left edge, and the first such line is scrolled into view on open.
- States: loading, load error with a retry button, and "No details were recorded for this job" when the endpoint answers 404. If the job leaves `needs-human` while the popup is open, a note says so.
- Behavior: opens on button click, Esc or the Close button closes it, focus returns to the button. Visible focus ring. At 400 px the popup is full width with 16 px gutters. No animation.
- The CSP forbids inline styles, so all styling is in `dashboard.css` and any dynamic size goes through the CSSOM like the token bar.

## Testing

- `factory-engine`: for each `kind`, a park produces `park.json` with the right kind, parts and `next`; a long part is cut at 200 KB with `truncated`; a park still happens when the file cannot be written; a missing source file leaves the part out.
- `factory-core`: the board round trip keeps `park_kind` and `park_at`, and an old board without them loads.
- `factory-cli`: `factory status` prints the reason, the excerpt and the `next` lines for a parked job, and only the reason for an old one.
- `factory-server`: the endpoint returns the known fields only, 404 for a missing file, 422 for a broken one, 405 for POST, and rejects an id with path characters and a job folder outside the sdd root.
- Dashboard JS tests (`tests/board.test.js` style): the button shows only with `has_park`; the popup builds its sections and copy chips; error lines get the marker class; the 404 and error states render.
- A manual look in a real window (light, dark and phone width) is left to the user, as agents cannot see a browser.

## Acceptance criteria

- Every park site writes a `park.json` with a `kind`, the reason, a `next` list and the evidence parts listed in the table.
- The board gains `park_kind` and `park_at` only, and an old board still loads in the driver and in `factory-server`.
- `GET /api/jobs/<id>/park` returns the sanitized evidence, with the status codes above, and cannot read outside the sdd folder.
- A `needs-human` card with details shows a "See what went wrong" button that opens a popup with the reason, the evidence parts and the suggested commands.
- Lines that start with `error`, `FAIL`, `FAILED` or `[blocker]` are marked, and the first one is scrolled into view.
- `factory status` shows the reason, a short excerpt and the next commands for a parked job.
- The popup works by keyboard, at phone width, and in light and dark themes.
- No LLM calls are added, and the polled `/api/state` does not carry the evidence text.
- `.claude/software-factory.md` documents `park.json`, the endpoint and the popup.
