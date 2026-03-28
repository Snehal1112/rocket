# cURL Paste Import Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let users paste a cURL command into Rocket's request URL field
and convert it into a structured request with best-effort support for
advanced pieces like multipart files and cookies.

**Architecture:** Detect cURL paste in the frontend request-builder URL
input, parse it into a normalized request model with warnings, and apply
the parsed fields to the active request in one pass. Keep normal URL
paste behavior unchanged for non-cURL input.

**Tech Stack:** React 19, TypeScript, Vite, Vitest

---

### Task 1: Add failing parser tests for representative cURL commands

**Files:**
- Create: `frontend/src/lib/curl-parser.test.ts`
- Create: `frontend/src/lib/curl-parser.ts`
- Check: `frontend/src/types/index.ts`

**Step 1: Write the failing tests**

Add tests for:

- simple JSON cURL with `-X`, `-H`, `--data-raw`
- multipart cURL with `-F field=value` and `-F file=@/tmp/a.png`
- basic auth via `-u`
- cookie import via `-b`
- unsupported flags producing warnings
- shell-like values being preserved as literals when not safely
  resolvable

The parser should return a normalized object, not mutate UI state.

**Step 2: Run the focused tests**

Run:

```bash
cd frontend && yarn test src/lib/curl-parser.test.ts
```

Expected: FAIL because the parser does not exist yet.

**Step 3: Commit test-only updates if needed**

```bash
git add frontend/src/lib/curl-parser.test.ts frontend/src/lib/curl-parser.ts
git commit -m "test(parser): cover curl paste import"
```

### Task 2: Implement the cURL parser and normalization layer

**Files:**
- Modify: `frontend/src/lib/curl-parser.ts`
- Test: `frontend/src/lib/curl-parser.test.ts`
- Modify if needed: `frontend/src/types/index.ts`

**Step 1: Implement minimal parsing**

Create a parser that:

- tokenizes cURL command text safely enough for common quoted values
- extracts recognized flags
- builds a normalized Rocket import result
- records unsupported or ambiguous parts as warnings

Keep scope to the supported flag list from the design. Do not try to
execute shell syntax.

**Step 2: Infer Rocket request fields**

Normalize into:

- method
- url
- headers
- query params
- auth
- body type and content
- multipart form-data items
- cookie import representation
- warnings

**Step 3: Run parser tests**

Run:

```bash
cd frontend && yarn test src/lib/curl-parser.test.ts
```

Expected: PASS.

**Step 4: Commit**

```bash
git add frontend/src/lib/curl-parser.ts frontend/src/lib/curl-parser.test.ts frontend/src/types/index.ts
git commit -m "feat(parser): add curl import parsing"
```

### Task 3: Wire cURL paste detection into the request URL field

**Files:**
- Modify: `frontend/src/components/request-builder/VariableAwareUrlInput.tsx`
- Modify: `frontend/src/components/request-builder/RequestBuilderToolbar.tsx`
- Modify: `frontend/src/components/request-builder/useRequestBuilderState.ts`
- Create or Modify: `frontend/src/components/request-builder/VariableAwareUrlInput.test.tsx`

**Step 1: Write the failing UI tests**

Add tests for:

- normal URL paste keeps existing behavior
- cURL paste triggers import flow instead of raw URL insertion
- parsed request fields replace stale request state
- warnings are surfaced when parser reports unsupported flags

**Step 2: Implement paste interception**

Update the URL input flow so:

- plain URL paste still sets URL normally
- cURL paste is detected and parsed
- successful parse applies request fields to the active request state
- parse failure shows an error and leaves state unchanged

Add the smallest possible warning UI that fits current patterns.

**Step 3: Run the focused component tests**

Run:

```bash
cd frontend && yarn test src/components/request-builder/VariableAwareUrlInput.test.tsx
```

Expected: PASS.

**Step 4: Commit**

```bash
git add frontend/src/components/request-builder/VariableAwareUrlInput.tsx frontend/src/components/request-builder/RequestBuilderToolbar.tsx frontend/src/components/request-builder/useRequestBuilderState.ts frontend/src/components/request-builder/VariableAwareUrlInput.test.tsx
git commit -m "feat(request-builder): import curl on paste"
```

### Task 4: Verify advanced import behavior end to end

**Files:**
- Check: `frontend/src/lib/curl-parser.ts`
- Check: `frontend/src/components/request-builder/useRequestBuilderState.ts`
- Check: `frontend/src/components/request-builder/VariableAwareUrlInput.tsx`

**Step 1: Run targeted automated tests**

Run:

```bash
cd frontend && yarn test src/lib/curl-parser.test.ts src/components/request-builder/VariableAwareUrlInput.test.tsx
```

Expected: PASS.

**Step 2: Manually verify real examples**

Run:

```bash
cd frontend && yarn dev --host 127.0.0.1 --port 5173
```

Paste into the URL field:

- a simple JSON cURL
- a multipart cURL with file parts
- a cURL with cookies
- a cURL with unsupported flags

Confirm:

- request fields are populated correctly
- unsupported pieces generate warnings
- normal URL paste still works

**Step 3: Commit final adjustments**

```bash
git add frontend/src/lib/curl-parser.ts frontend/src/lib/curl-parser.test.ts frontend/src/components/request-builder/VariableAwareUrlInput.tsx frontend/src/components/request-builder/RequestBuilderToolbar.tsx frontend/src/components/request-builder/useRequestBuilderState.ts frontend/src/components/request-builder/VariableAwareUrlInput.test.tsx frontend/src/types/index.ts
git commit -m "feat(request-builder): support curl paste import"
```
