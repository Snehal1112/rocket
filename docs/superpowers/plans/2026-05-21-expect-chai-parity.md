# Expect — Full Chai API Parity — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled `expect()` in `bootstrap.js` with delegation to the already-bundled Chai 4.3.8 UMD, add `@types/chai` for full Monaco IntelliSense, and expand the snippets sidebar to cover the full Chai API.

**Architecture:** `bootstrap.js` calls `require('chai')` (already wired) and assigns `chai.expect` as the global `expect`. A thin `toBe` patch preserves the jest-style alias. `rok-types.ts` replaces its hand-rolled `ChaiAssertion` interface with the official `Chai.ExpectStatic` type, inlined from `@types/chai` so Monaco can resolve it without a `/// <reference>` directive.

**Tech Stack:** JavaScript (bootstrap.js), TypeScript (rok-types.ts), Chai 4.3.8 (already bundled at `crates/rocket-infra/src/scripting/modules/chai.js`), `@types/chai` v4 (new devDependency).

**Spec:** `docs/superpowers/specs/2026-05-21-expect-chai-parity-design.md`

---

## File Map

| File | Action | What changes |
|---|---|---|
| `crates/rocket-infra/src/scripting/bootstrap.js` | Modify | Replace lines 84–123 (hand-rolled `expect`) with Chai delegation |
| `src/components/editor/rok-types.ts` | Modify | Replace `ChaiAssertion` interface + expand snippets |
| `package.json` | Modify | Add `@types/chai` devDependency |

No Rust files change. No IPC changes. No test result display changes.

---

## Task 1: Install `@types/chai` and verify TypeScript sees it

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Install the package**

```bash
yarn add -D @types/chai@^4.3.20
```

Expected output: something like `success Saved 1 new dependency` and `@types/chai@4.x.x` in devDependencies.

- [ ] **Step 2: Verify TypeScript resolves the Chai namespace**

```bash
cd /home/numericlabs/data/rocket/rocket
echo "const x: Chai.ExpectStatic = undefined as any;" > /tmp/chai-check.ts
yarn tsc --noEmit --skipLibCheck false /tmp/chai-check.ts 2>&1 | head -20
rm /tmp/chai-check.ts
```

Expected: no errors (or only "cannot find module" for the temp file path — the key is that `Chai.ExpectStatic` resolves). If it errors with `Cannot find namespace 'Chai'`, confirm `@types/chai` landed in `node_modules/@types/chai/index.d.ts`:

```bash
ls node_modules/@types/chai/index.d.ts
```

- [ ] **Step 3: Commit**

```bash
git add package.json yarn.lock
git commit -m "chore(scripts): add @types/chai devDependency for expect IntelliSense"
```

---

## Task 2: Replace hand-rolled `expect()` in bootstrap.js with Chai delegation

**Files:**
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js:84-124`

- [ ] **Step 1: Open bootstrap.js and locate the expect block**

The hand-rolled block starts at line 84 with `function expect(actual) {` and ends at line 123 with the closing `}`. Line 124 is `globalThis.expect = expect;`.

- [ ] **Step 2: Replace the hand-rolled block**

Replace lines 84–124 with the following. Keep the `rok.expect` alias line (currently line 138) — it stays unchanged.

```js
// ── test() + expect() ────────────────────────────────────────────────────────
// Delegate to bundled Chai for full API parity.
const _chai = require('chai');
globalThis.expect = function(actual) {
  const assertion = _chai.expect(actual);
  // jest-style alias — not in Chai natively.
  assertion.toBe = (expected) => _chai.expect(actual).to.equal(expected);
  return assertion;
};
```

The full bootstrap.js section from the comment line onward should look like this after the edit:

```js
// ── test() + expect() ────────────────────────────────────────────────────────
// Delegate to bundled Chai for full API parity.
const _chai = require('chai');
globalThis.expect = function(actual) {
  const assertion = _chai.expect(actual);
  // jest-style alias — not in Chai natively.
  assertion.toBe = (expected) => _chai.expect(actual).to.equal(expected);
  return assertion;
};

globalThis.test = function(name, fn) {
  Deno.core.ops.op_test_run(name);
  try {
    fn();
    Deno.core.ops.op_test_pass(name);
  } catch (e) {
    Deno.core.ops.op_test_fail(name, String(e));
  }
};

// rok.test / rok.expect aliases so both calling styles work.
rok.test   = globalThis.test;
rok.expect = globalThis.expect;
```

- [ ] **Step 3: Verify bootstrap.js compiles (Rust check)**

`bootstrap.js` is embedded via `include_str!()` — a Rust compile check confirms no syntax errors in the embedding:

```bash
cargo check -p rocket-infra
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/scripting/bootstrap.js
git commit -m "feat(scripts): delegate expect() to bundled Chai for full API parity"
```

---

## Task 3: Update TypeScript type definitions in rok-types.ts

**Files:**
- Modify: `src/components/editor/rok-types.ts:190-212`

The current `TEST_DEFS` string contains a hand-rolled `ChaiAssertion` interface (lines 194–211). We replace it with the official Chai types.

**Why inline instead of `/// <reference>`:** Monaco's `addExtraLib` executes in an isolated type context — it cannot resolve `/// <reference types="chai" />` against the project's `node_modules`. We must inline the relevant type declarations.

- [ ] **Step 1: Extract the Chai type definitions string**

Run this to get the content of `@types/chai`:

```bash
wc -l node_modules/@types/chai/index.d.ts
```

The file is large (~1400 lines). We don't need to inline all of it — Monaco only needs the types reachable from `expect`. The key types are `ExpectStatic`, `Assertion`, and `Assert`. A clean approach: inline the entire `@types/chai/index.d.ts` content into a TypeScript constant.

- [ ] **Step 2: Create a helper script to read the chai types**

```bash
node -e "const fs=require('fs'); const s=fs.readFileSync('node_modules/@types/chai/index.d.ts','utf8'); console.log(s.length + ' chars, ' + s.split('\n').length + ' lines');"
```

This confirms size. It will be ~50KB. That is acceptable for an `addExtraLib` string — Monaco handles large definition files.

- [ ] **Step 3: Replace `TEST_DEFS` in `src/components/editor/rok-types.ts`**

Replace the existing `const TEST_DEFS = \`...\`` block (lines 190–212) with:

```typescript
// Read at module init time so the string is available synchronously.
// We inline chai types rather than using /// <reference> because Monaco's
// addExtraLib context cannot resolve node_modules types via reference directives.
import chaiTypeDefsRaw from '@types/chai/index.d.ts?raw';

const CHAI_TYPE_DEFS = chaiTypeDefsRaw
  // Strip triple-slash references that Monaco can't resolve.
  .replace(/\/\/\/\s*<reference[^>]*>\s*\n/g, '');

const TEST_DEFS = `
${CHAI_TYPE_DEFS}

/** Register a named assertion block. Each block runs independently. */
declare function test(name: string, fn: () => void): void;

/** Full Chai expect — chain assertions with .to.equal(), .to.have.property(), .to.match(), etc. */
declare const expect: Chai.ExpectStatic;
`;
```

**Important:** Vite supports `?raw` imports for `.d.ts` files. Check that `@types/chai/index.d.ts` resolves — if not (Vite may not resolve `@types` paths with `?raw`), use the fallback below.

**Fallback if `?raw` import fails:** Copy the chai type definitions to the `src` tree and import from there:

```bash
cp node_modules/@types/chai/index.d.ts src/components/editor/chai-types.d.ts.txt
```

Then import:

```typescript
import chaiTypeDefsRaw from './chai-types.d.ts.txt?raw';
```

- [ ] **Step 4: Verify TypeScript compiles cleanly**

```bash
yarn tsc --noEmit
```

Expected: no errors. If you see `Cannot find module '@types/chai/index.d.ts'` from the `?raw` import, use the fallback path from Step 3.

- [ ] **Step 5: Commit**

```bash
git add src/components/editor/rok-types.ts
git commit -m "feat(scripts): replace hand-rolled ChaiAssertion with @types/chai for full IntelliSense"
```

---

## Task 4: Expand snippets sidebar entries in rok-types.ts

**Files:**
- Modify: `src/components/editor/rok-types.ts` — the `ROK_SNIPPETS` array (lines 22–108)

- [ ] **Step 1: Expand Common Tests section**

Replace the `common-tests` items array (currently 7 items) with these 12 items:

```typescript
items: [
  {
    label: 'Status is 200',
    kind: 'template',
    code: `test("Status is 200", () => {\n  expect(res.getStatus()).to.equal(200);\n});`,
  },
  {
    label: 'Status is 2xx',
    kind: 'template',
    code: `test("Status is 2xx", () => {\n  expect(res.getStatus()).to.be.within(200, 299);\n});`,
  },
  {
    label: 'Response time < 200ms',
    kind: 'template',
    code: `test("Response time < 200ms", () => {\n  expect(res.getResponseTime()).to.be.below(200);\n});`,
  },
  {
    label: 'Body has property',
    kind: 'template',
    code: `test("Body has property", () => {\n  const body = res.getBody();\n  expect(body).to.have.property("key");\n});`,
  },
  {
    label: 'Body equals value',
    kind: 'template',
    code: `test("Body equals value", () => {\n  const body = res.getBody();\n  expect(body.key).to.equal("value");\n});`,
  },
  {
    label: 'Body deep equals',
    kind: 'template',
    code: `test("Body deep equals", () => {\n  const body = res.getBody();\n  expect(body).to.deep.equal({ key: "value" });\n});`,
  },
  {
    label: 'Body is array',
    kind: 'template',
    code: `test("Body is array", () => {\n  expect(res.getBody()).to.be.an("array");\n});`,
  },
  {
    label: 'Body is not empty',
    kind: 'template',
    code: `test("Body is not empty", () => {\n  expect(res.getBody()).to.not.be.empty;\n});`,
  },
  {
    label: 'Header exists',
    kind: 'template',
    code: `test("Header exists", () => {\n  expect(res.getHeader("content-type")).to.exist;\n});`,
  },
  {
    label: 'Header equals value',
    kind: 'template',
    code: `test("Header equals value", () => {\n  expect(res.getHeader("content-type")).to.include("application/json");\n});`,
  },
  {
    label: 'Status is 404',
    kind: 'template',
    code: `test("Status is 404", () => {\n  expect(res.getStatus()).to.equal(404);\n});`,
  },
  {
    label: 'Body matches regex',
    kind: 'template',
    code: `test("Body matches regex", () => {\n  const body = res.getBody({ raw: true });\n  expect(body).to.match(/pattern/);\n});`,
  },
],
```

- [ ] **Step 2: Replace the `expect` API Reference subgroup**

Replace the `expect` subgroup items (currently 7 items) with these comprehensive entries:

```typescript
{
  id: 'expect',
  label: 'expect',
  items: [
    // Equality
    { label: '.to.equal(value)', kind: 'expression', code: '.to.equal(value)' },
    { label: '.to.deep.equal(value)', kind: 'expression', code: '.to.deep.equal(value)' },
    { label: '.to.eql(value)', kind: 'expression', code: '.to.eql(value)' },
    // Existence / truthiness
    { label: '.to.exist', kind: 'expression', code: '.to.exist' },
    { label: '.to.be.ok', kind: 'expression', code: '.to.be.ok' },
    { label: '.to.be.true', kind: 'expression', code: '.to.be.true' },
    { label: '.to.be.false', kind: 'expression', code: '.to.be.false' },
    { label: '.to.be.null', kind: 'expression', code: '.to.be.null' },
    { label: '.to.be.undefined', kind: 'expression', code: '.to.be.undefined' },
    // Type
    { label: '.to.be.a("type")', kind: 'expression', code: '.to.be.a("type")' },
    { label: '.to.be.an("array")', kind: 'expression', code: '.to.be.an("array")' },
    { label: '.to.be.instanceof(Constructor)', kind: 'expression', code: '.to.be.instanceof(Constructor)' },
    // Numeric
    { label: '.to.be.above(n)', kind: 'expression', code: '.to.be.above(n)' },
    { label: '.to.be.below(n)', kind: 'expression', code: '.to.be.below(n)' },
    { label: '.to.be.within(min, max)', kind: 'expression', code: '.to.be.within(min, max)' },
    { label: '.to.be.closeTo(n, delta)', kind: 'expression', code: '.to.be.closeTo(n, delta)' },
    { label: '.to.be.at.least(n)', kind: 'expression', code: '.to.be.at.least(n)' },
    { label: '.to.be.at.most(n)', kind: 'expression', code: '.to.be.at.most(n)' },
    // String / array / object
    { label: '.to.include("str")', kind: 'expression', code: '.to.include("str")' },
    { label: '.to.have.length(n)', kind: 'expression', code: '.to.have.length(n)' },
    { label: '.to.have.property("key")', kind: 'expression', code: '.to.have.property("key")' },
    { label: '.to.have.property("key", value)', kind: 'expression', code: '.to.have.property("key", value)' },
    { label: '.to.have.own.property("key")', kind: 'expression', code: '.to.have.own.property("key")' },
    { label: '.to.have.keys("a", "b")', kind: 'expression', code: '.to.have.keys("a", "b")' },
    { label: '.to.have.members([...])', kind: 'expression', code: '.to.have.members([])' },
    { label: '.to.match(/regex/)', kind: 'expression', code: '.to.match(/regex/)' },
    { label: '.to.be.empty', kind: 'expression', code: '.to.be.empty' },
    { label: '.to.satisfy(fn)', kind: 'expression', code: '.to.satisfy((val) => val > 0)' },
    // Negation
    { label: '.to.not.equal(value)', kind: 'expression', code: '.to.not.equal(value)' },
    { label: '.to.not.have.property("key")', kind: 'expression', code: '.to.not.have.property("key")' },
    { label: '.to.not.include("str")', kind: 'expression', code: '.to.not.include("str")' },
    { label: '.to.not.be.null', kind: 'expression', code: '.to.not.be.null' },
    { label: '.to.not.be.undefined', kind: 'expression', code: '.to.not.be.undefined' },
    { label: '.to.not.be.empty', kind: 'expression', code: '.to.not.be.empty' },
    // Function
    { label: '.to.throw()', kind: 'expression', code: '.to.throw()' },
  ],
},
```

- [ ] **Step 3: Verify TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Run lint**

```bash
yarn check
```

Expected: no errors or warnings.

- [ ] **Step 5: Commit**

```bash
git add src/components/editor/rok-types.ts
git commit -m "feat(scripts): expand snippets sidebar with full Chai matcher set"
```

---

## Task 5: Manual verification

**No files changed in this task — verification only.**

- [ ] **Step 1: Start the dev server**

```bash
yarn tauri dev
```

Wait for the window to appear.

- [ ] **Step 2: Verify existing matchers still work**

Open any request → Scripts tab → Tests sub-tab. Type and run:

```js
test("basic equality", () => {
  expect(1 + 1).to.equal(2);
});

test("toBe alias", () => {
  expect("hello").toBe("hello");
});

test("within range", () => {
  expect(5).to.be.within(1, 10);
});
```

Expected: all three tests pass (green checkmarks in the Tests panel after sending the request).

- [ ] **Step 3: Verify new Chai matchers work**

```js
test("deep equal", () => {
  expect({ a: 1 }).to.deep.equal({ a: 1 });
});

test("match regex", () => {
  expect("hello world").to.match(/hello/);
});

test("not be empty", () => {
  expect([1, 2, 3]).to.not.be.empty;
});

test("have members", () => {
  expect([1, 2, 3]).to.have.members([3, 1, 2]);
});

test("closeTo", () => {
  expect(3.14).to.be.closeTo(3, 0.2);
});
```

Expected: all five pass.

- [ ] **Step 4: Verify negation works**

```js
test("not equal", () => {
  expect(1).to.not.equal(2);
});

test("not have property", () => {
  expect({ a: 1 }).to.not.have.property("b");
});
```

Expected: both pass.

- [ ] **Step 5: Verify Monaco IntelliSense**

In the Tests tab editor, type `expect(res.getBody()).to.` and pause. Verify that Monaco autocomplete shows a rich list of Chai matchers (deep, equal, have, match, etc.) — not just the 7 from the old hand-rolled interface.

- [ ] **Step 6: Verify snippets sidebar expanded entries**

In the snippets sidebar, open the `expect` subgroup under API Reference. Confirm it now shows the full expanded list including `.to.deep.equal`, `.to.match`, `.to.not.be.empty`, etc.

- [ ] **Step 7: Final lint + type check**

```bash
yarn tsc --noEmit && yarn check
```

Expected: clean.

---

## Verification summary

```bash
cargo check -p rocket-infra   # bootstrap.js embedding
yarn tsc --noEmit              # TypeScript
yarn check                     # Biome lint
```

All three must pass before the feature is considered complete.
