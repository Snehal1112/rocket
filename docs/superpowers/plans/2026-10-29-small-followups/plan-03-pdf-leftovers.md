# Remove the print-to-PDF leftovers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the comments and the capability entry that describe a print-to-PDF window that does not exist.

**Architecture:** Drop `pdf-*` from the default capability's window list, pin that with a small file-reading test, and rewrite the two comments in `export_service.rs` so they say what the code does.

**Tech Stack:** Tauri capability JSON, Vitest (reads the JSON with `node:fs`), Rust comments.

**Spec:** GitHub issue #126 (https://github.com/Snehal1112/rocket/issues/126), a follow-up of `docs/superpowers/specs/2026-10-29-load-test-report-escape-design.md`. The ticket is one line, so the decisions below are this plan's own calls.

## Global Constraints

- Files touched: `src-tauri/capabilities/default.json`, `crates/rocket-app/src/export_service.rs` (two comments only), and one new test file `src/lib/__tests__/tauri-capabilities.test.ts`.
- `src-tauri/**` matches the factory's risk rules. If this plan is ever run by the factory, expect the job to stop at `needs-human` after review. Run it natively or approve it by hand.
- No behaviour change in Rust. `pdf_returns_html_bytes` must keep passing unchanged.
- Cargo commands carry `-j4` and `-p rocket-app`. Frontend commands use Bun: `bun run test <pattern>`, `bun run tsc --noEmit`, `bun run check`. (The project docs say yarn; the owner's rule is Bun.)
- No formatting edits elsewhere in the JSON or the Rust file.

## Decisions (this plan's calls)

- The ticket allows "remove, or build the PDF flow with a locked-down window". This plan removes. Nothing in `src/` or `src-tauri/src/` creates a window that matches `pdf-*` (checked with a grep on 2026-10-29), so the entry only widens what a future window could do: `fs:allow-write-file`, `shell:default`, `opener:default`.
- The test lives in the frontend suite because it only reads a JSON file. An integration test in `src-tauri/tests` would compile the whole app and V8.
- The "PDF" export format still returns HTML bytes with the extension `pdf`. That is a separate user-facing problem and is not changed here.

## Review Focus

1. The default capability names only the `main` window. Pinned by `grants the default capability to the main window only`.
2. The list has no wildcard entry, so a future window cannot join by name. Pinned by `lists no wildcard window in the default capability`.
3. The file still parses as JSON after the edit. Pinned by both tests, which parse it first.
4. The Rust behaviour of the `Pdf` format is unchanged. Pinned by the existing `pdf_returns_html_bytes`.
5. Nothing else refers to `pdf-*`. Pinned by a final grep in Step 5.

## File Structure

- Create `src/lib/__tests__/tauri-capabilities.test.ts`: reads `src-tauri/capabilities/default.json` and checks the `windows` list.
- Modify `src-tauri/capabilities/default.json`: `"windows": ["main"]`.
- Modify `crates/rocket-app/src/export_service.rs`: the doc comment on `ExportService::export` and the doc comment above `pdf_returns_html_bytes`.

---

### Task 1: Pin the capability's windows and remove the leftovers

**Files:**
- Create: `src/lib/__tests__/tauri-capabilities.test.ts`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `crates/rocket-app/src/export_service.rs` (two comments)

**Interfaces:**
- Consumes: nothing from other plans.
- Produces: nothing other plans rely on.

- [ ] **Step 1: Read the code**

Read `src-tauri/capabilities/default.json` (the `windows` array has `"main"` and `"pdf-*"`), the doc comment on `ExportService::export` (it says the Tauri command layer renders the HTML through the webview's print-to-PDF API), and the doc comment above `pdf_returns_html_bytes`. Read `src/lib/__tests__/curl-generator.test.ts` for how this repo's Vitest files import Node modules.

- [ ] **Step 2: Write the failing tests**

```ts
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

function defaultCapability(): { windows: string[]; permissions: string[] } {
  const file = resolve(__dirname, '../../../src-tauri/capabilities/default.json');
  return JSON.parse(readFileSync(file, 'utf8'));
}

describe('default Tauri capability', () => {
  it('grants the default capability to the main window only', () => {
    expect(defaultCapability().windows).toEqual(['main']);
  });

  it('lists no wildcard window in the default capability', () => {
    expect(defaultCapability().windows.filter((w) => w.includes('*'))).toEqual([]);
  });
});
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `bun run test tauri-capabilities`
Expected: the first test fails with `expected [ 'main', 'pdf-*' ] to deeply equal [ 'main' ]`, the second fails with `[ 'pdf-*' ]` not equal to `[]`.

- [ ] **Step 4: Make the edits**

In `src-tauri/capabilities/default.json` change `"windows"` to `["main"]`, written in the file's existing multi-line style. In `crates/rocket-app/src/export_service.rs` replace the two comment texts: on `export` say that for `Pdf` the bytes are the same HTML report with the extension `pdf`, and that nothing renders them to a PDF; above `pdf_returns_html_bytes` say that PDF is a passthrough that returns the HTML bytes. Do not change any code.

- [ ] **Step 5: Run the checks**

Run: `bun run test tauri-capabilities` (expected: 2 passed), `cargo test -j4 -p rocket-app export_service` (expected: all pass, including `pdf_returns_html_bytes`), `cargo check -j4 -p rocket-app`, `bun run check`. Then run `grep -rn 'pdf-\*\|print-to-PDF' src src-tauri/src src-tauri/capabilities crates/rocket-app/src` and expect no output. Run `git diff --stat` and confirm only the three listed files changed.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill, path-scoped: `git commit --only -- src/lib/__tests__/tauri-capabilities.test.ts src-tauri/capabilities/default.json crates/rocket-app/src/export_service.rs`. Subject: `chore(tauri): drop the unused pdf window capability`. Footer: `Fixes: #126`.

Next plan: docs/superpowers/plans/2026-10-29-small-followups/plan-04-http-create-fallback.md
