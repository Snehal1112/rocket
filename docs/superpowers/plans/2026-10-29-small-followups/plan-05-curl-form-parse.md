# Parse curl --form on paste Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A multipart command that Rocket copied with "Copy as cURL" pastes back as the same form-data body.

**Architecture:** `parseCurl` learns `-F`, `--form` and `--form-string` and returns a form-data body with typed entries. A small exported function turns the parsed body into the request `Body`, and the paste handler in `RequestPanel` uses it. The generator in `curl-generator.ts` already writes `--form 'key=@path'` for files and `--form-string 'key=value'` for text.

**Tech Stack:** TypeScript, Vitest, React (`RequestPanel`).

**Spec:** GitHub issue #119 (https://github.com/Snehal1112/rocket/issues/119), a follow-up of #118 (job 032, `curl-copy-file-read`). The ticket is one line, so the decisions below are this plan's own calls.

## Global Constraints

- Files touched: `src/lib/curl-parser.ts`, `src/components/request/RequestPanel.tsx` (only `handleCurlImport`), and the new test file `src/lib/__tests__/curl-parser.test.ts`.
- Frontend commands use Bun (the owner's rule; the project docs say yarn): `bun run test curl-parser`, `bunx tsc --noEmit`, `bun run check`. `bun run check` is read-only Biome; to auto-fix, run `bunx biome check --write <files>` on the touched files only.
- Hard rules from `CLAUDE.md`: shadcn/ui primitives, lucide-react icons, narrow Zustand selectors. This plan adds no UI.
- No formatting edits to other files.
- Existing behaviour stays: `-d`, `--data`, `--data-raw`, `--data-binary` still give a `json`, `xml` or `text` body; `-u`, `-A`, `-H`, `-X` are unchanged.

## Decisions (this plan's calls)

- Entry rules follow what the generator writes and curl's documented syntax:
  - `-F` and `--form` take `name=value`. A value that starts with `@` is a file: the path is the text after `@` up to the first `;`, and `;type=<mime>` sets `contentType`. Other `;` options (`filename=`) are dropped.
  - A value that starts with `<` (curl reads the file as text) stays a text entry with the value as written. Rocket cannot read the file on paste.
  - `--form-string` is always a text entry with the value exactly as written, even if it starts with `@` or `<`.
  - The name is the text before the first `=`. A value may contain more `=` signs. An argument with no `=` is ignored.
  - Every entry is `enabled: true`.
- When a command has both `-d` and form flags, the form body wins. Real curl refuses that mix, so this is only a safe choice.
- The method is inferred as `POST` for a form body, as for `-d`. An explicit `-X` wins.
- A pasted `Content-Type` header is left in the headers as written. This plan does not drop or rewrite it.
- The parsed body type grows a `formdata` mode with a `formData` list. `bodyFromParsedCurl` is the single place that turns it into the request `Body`.

## Review Focus

1. A text field: `-F 'name=Ada'` becomes one text entry. Pinned by `parses a text --form field into a form-data body`.
2. A file field: `-F 'file=@/tmp/a.png;type=image/png'` becomes a file entry with the path and `contentType`. Pinned by `parses a file --form field with its type`.
3. A literal that looks like a file: `--form-string 'note=@not-a-file'` stays text. Pinned by `keeps --form-string values as text, even when they start with @`.
4. The URL after the flag: `curl -F a=1 https://x.test/up` still finds the URL. Pinned by the first test, which asserts `url`.
5. A copied command pastes back as the same body. Pinned by `round-trips a Copy as cURL form-data body`.

## File Structure

- Modify `src/lib/curl-parser.ts`: type, parsing branch, `bodyFromParsedCurl`.
- Create `src/lib/__tests__/curl-parser.test.ts`.
- Modify `src/components/request/RequestPanel.tsx`: `handleCurlImport` uses `bodyFromParsedCurl`.

---

### Task 1: Parse `--form`, `-F` and `--form-string`

**Files:**
- Modify: `src/lib/curl-parser.ts`
- Create: `src/lib/__tests__/curl-parser.test.ts`

**Interfaces:**
- Consumes: `FormDataEntry` from `@/lib/tauri-api` (`{ key: string; value: string; entryType: 'text' | 'file'; enabled: boolean; contentType?: string }`).
- Produces: `ParsedCurl['body']` becomes `{ mode: 'json' | 'xml' | 'text' | 'formdata'; content: string; formData?: FormDataEntry[] }`. A form body has `mode: 'formdata'`, `content: ''` and a non-empty `formData`.

- [ ] **Step 1: Read the code**

Read `src/lib/curl-parser.ts` (the `while` loop in `parseCurl`, where unknown flags skip the next token), `src/lib/curl-generator.ts` (`buildBodyParts`, the `formdata` case, and `shellQuote`), and `src/lib/__tests__/curl-generator.test.ts` for the test style.

- [ ] **Step 2: Write the failing tests**

One full test; the others are in the table.

```ts
import { describe, expect, it } from 'vitest';
import { parseCurl } from '@/lib/curl-parser';

describe('parseCurl form data', () => {
  it('parses a text --form field into a form-data body', () => {
    const parsed = parseCurl("curl -F 'name=Ada' https://x.test/up");
    expect(parsed?.url).toBe('https://x.test/up');
    expect(parsed?.method).toBe('POST');
    expect(parsed?.body).toEqual({
      mode: 'formdata',
      content: '',
      formData: [{ key: 'name', value: 'Ada', entryType: 'text', enabled: true }],
    });
  });
});
```

| Test | Command | Assertion on `body.formData` or other |
|---|---|---|
| `parses a file --form field with its type` | `curl --form 'file=@/tmp/a.png;type=image/png' https://x.test/up` | one entry `{ key: 'file', value: '/tmp/a.png', entryType: 'file', enabled: true, contentType: 'image/png' }` |
| `drops other ; options from a file field` | `curl -F 'f=@a.txt;filename=b.txt' https://x.test` | `value` is `a.txt`, no `contentType` key |
| `keeps --form-string values as text, even when they start with @` | `curl --form-string 'note=@not-a-file' --form-string 'n2=<x' https://x.test` | two text entries with values `@not-a-file` and `<x` |
| `keeps equals signs inside a value` | `curl -F 'token=a=b' https://x.test` | `{ key: 'token', value: 'a=b', entryType: 'text' }` |
| `ignores a --form argument without an equals sign` | `curl -F 'oops' -F 'a=1' https://x.test` | exactly one entry, key `a` |
| `lets the form body win over -d` | `curl -d 'x=1' -F 'a=1' https://x.test` | `body.mode` is `formdata` |
| `lets an explicit -X win over the inferred POST` | `curl -X PUT -F 'a=1' https://x.test` | `method` is `PUT` |
| `keeps -d parsing as before` | `curl -d '{"a":1}' https://x.test` | `body` equals `{ mode: 'json', content: '{"a":1}' }` and `method` is `POST` |

- [ ] **Step 3: Run the tests and see them fail**

Run: `bun run test curl-parser`
Expected: the form tests fail (`body` is undefined or `url` is wrong, because the generic branch skips the value token). `keeps -d parsing as before` passes.

- [ ] **Step 4: Implement the form branch**

In `src/lib/curl-parser.ts` add `import type { FormDataEntry } from '@/lib/tauri-api';`, widen `ParsedCurl['body']` as in the Interfaces block, and add a `formData: FormDataEntry[]` accumulator in `parseCurl`. Add one branch before the generic `token.startsWith('-')` branch for `-F` and `--form` and one for `--form-string`; both read `tokens[++i]`. Put the entry parsing in a private function `parseFormEntry(arg: string, literal: boolean): FormDataEntry | null` that applies the Decisions rules. After the loop, when `formData.length > 0` build `body = { mode: 'formdata', content: '', formData }` and infer `POST` when no method was given. Keep the existing `-d` path for the other case.

- [ ] **Step 5: Run the tests and see them pass**

Run: `bun run test curl-parser`, `bunx tsc --noEmit`, `bun run check`
Expected: 9 curl-parser tests pass (the full example plus the 8 in the table), tsc and Biome are clean.

---

### Task 2: Round trip and the paste handler

**Files:**
- Modify: `src/lib/curl-parser.ts` (add `bodyFromParsedCurl`)
- Modify: `src/components/request/RequestPanel.tsx` (`handleCurlImport`)
- Test: `src/lib/__tests__/curl-parser.test.ts`

**Interfaces:**
- Consumes: Task 1's `ParsedCurl['body']`; `generateCurlCommand(resolved: ResolvedRequestFields, method: string): string` from `@/lib/curl-generator`; `BodyState` from `@/types/pane-types`.
- Produces: `export function bodyFromParsedCurl(body: ParsedCurl['body']): BodyState | undefined`, where `BodyState` is the editor's body state in `src/types/pane-types.ts` (`content: string` is required and form rows are `KeyValueEntry` with a required `id`). For `undefined` it returns `undefined`. For a form body it returns `{ mode: 'formdata', content: '', formData: <entries, each with id: crypto.randomUUID()> }`. For other modes it returns `{ mode, content, formData: [] }`, which is what `handleCurlImport` builds today.

- [ ] **Step 1: Read the code**

Read `handleCurlImport` in `src/components/request/RequestPanel.tsx` (the `if (parsed.body)` block) and `baseResolved` in `src/lib/__tests__/curl-generator.test.ts`.

- [ ] **Step 2: Write the failing tests**

Add to `curl-parser.test.ts` (import `generateCurlCommand` and the `ResolvedRequestFields` type; copy the fields of `baseResolved` into a local helper):

```ts
it('round-trips a Copy as cURL form-data body', () => {
  const entries = [
    { key: 'name', value: "it's Ada", entryType: 'text' as const, enabled: true },
    { key: 'file', value: 'docs/a b.png', entryType: 'file' as const, enabled: true },
  ];
  const cmd = generateCurlCommand(
    localResolved({ body: { mode: 'formdata', formData: entries } }),
    'POST',
  );
  expect(parseCurl(cmd)?.body?.formData).toEqual(entries);
});
```

| Test | Setup | Assertion |
|---|---|---|
| `skips disabled entries when copying, so they do not come back` | a disabled third entry in the same body | the parsed list equals the first two entries only |
| `bodyFromParsedCurl returns undefined without a body` | `bodyFromParsedCurl(undefined)` | `undefined` |
| `bodyFromParsedCurl keeps form entries` | `bodyFromParsedCurl(parseCurl("curl -F 'a=1' https://x.test")?.body)` | `{ mode: 'formdata', content: '', formData: [{ id: expect.any(String), key: 'a', value: '1', entryType: 'text', enabled: true }] }` |
| `bodyFromParsedCurl gives each form row its own id` | two `-F` fields | the two row ids are different strings |
| `bodyFromParsedCurl keeps the text body shape` | `bodyFromParsedCurl({ mode: 'json', content: '{"a":1}' })` | `{ mode: 'json', content: '{"a":1}', formData: [] }` |

- [ ] **Step 3: Run the tests and see them fail**

Run: `bun run test curl-parser`
Expected: the `bodyFromParsedCurl` tests fail with `bodyFromParsedCurl is not a function`; the round-trip tests pass if Task 1 is correct (they guard the generator and parser together).

- [ ] **Step 4: Implement `bodyFromParsedCurl` and use it**

Add the exported function to `src/lib/curl-parser.ts` per the Interfaces block. In `handleCurlImport` replace the `patch.body = { mode: parsed.body.mode, content: parsed.body.content, formData: [] }` block with `patch.body = bodyFromParsedCurl(parsed.body)`, inside the same `if (parsed.body)` condition, and import the function next to the existing `ParsedCurl` type import. Leave the rest of the handler alone.

- [ ] **Step 5: Run the checks**

Run: `bun run test curl-parser`, `bun run test RequestPanel`, `bunx tsc --noEmit`, `bun run check`
Expected: all pass (15 curl-parser tests), `RequestPanel` tests unchanged and green. Run `git diff --stat` and confirm only the three listed files changed.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill, path-scoped: `git commit --only -- src/lib/curl-parser.ts src/lib/__tests__/curl-parser.test.ts src/components/request/RequestPanel.tsx`. Subject: `feat(curl): parse --form on paste`. Footer: `Fixes: #119`.

Next plan: docs/superpowers/plans/2026-10-29-small-followups/plan-06-clippy-backlog.md
