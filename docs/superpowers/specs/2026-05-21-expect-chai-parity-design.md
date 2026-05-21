# Expect — Full Chai API Parity

**Date:** 2026-05-21
**Status:** Approved
**Scope:** Replace the hand-rolled `expect()` in `bootstrap.js` with delegation to the bundled Chai library, expand TypeScript type definitions to cover the full Chai API, and add comprehensive snippets to the authoring sidebar.
**Out of scope:** Changes to `test()`, `rok.*`, `req.*`, `res.*`, test result display, IPC, or any Rust code.

---

## 1. Goals

- Give users full Chai assertion API parity — every matcher, every language chain, every negation.
- Zero ongoing maintenance: Chai logic lives in the bundled UMD, not in bootstrap.
- Keep RocketAPI-specific convenience shorthands (`toBe`) working.
- Update Monaco IntelliSense and the snippets sidebar to reflect the expanded API.

---

## 2. Why delegation beats expansion

The hand-rolled `expect()` covers ~12 matchers. Full Chai has 60+. Implementing them by hand means:
- 600–800 lines of JavaScript with subtle edge cases.
- Permanent divergence risk from real Chai behaviour.
- Every new matcher requires a bootstrap.js edit.

The Chai UMD is already bundled in `crates/rocket-infra/src/scripting/modules/chai.js` and `require('chai')` already works in the sandbox. Delegating costs 5 lines in bootstrap.js and gives permanent true parity.

---

## 3. bootstrap.js changes

### 3.1 Replace hand-rolled `expect()`

Remove lines 84–123 (the entire hand-rolled `function expect(actual) { ... }` block) and replace with:

```js
// Delegate expect() to bundled Chai for full API parity.
const _chai = require('chai');
globalThis.expect = function(actual) {
  const assertion = _chai.expect(actual);
  // jest-style alias — not in Chai natively.
  assertion.toBe = (expected) => _chai.expect(actual).to.equal(expected);
  return assertion;
};
rok.expect = globalThis.expect;
```

### 3.2 What is removed

- The hand-rolled `to.have.status(code)` convenience is dropped. It was checking `actual.status` which was never meaningful on a raw value. Users should write `expect(res.getStatus()).to.equal(200)` instead.
- All other hand-rolled matchers are superseded by Chai's native implementations.

### 3.3 What is kept

- `toBe(expected)` — jest-style strict equality alias, patched onto each assertion object. This is the only RocketAPI-specific extension.
- `rok.expect` alias continues to work.

---

## 4. Full Chai API surface now available

After this change, users have access to the complete Chai BDD interface:

**Equality**
- `.to.equal(val)` — strict (`===`)
- `.to.deep.equal(val)` — recursive deep equality
- `.to.eql(val)` — alias for deep.equal
- `.to.strictEqual(val)` — alias for equal

**Type / instance checks**
- `.to.be.a(type)` / `.to.be.an(type)` — typeof or instanceof
- `.to.be.instanceof(Constructor)`

**Truthiness / existence**
- `.to.be.ok` — truthy
- `.to.be.true` / `.to.be.false` / `.to.be.null` / `.to.be.undefined` / `.to.be.NaN`
- `.to.exist` — not null and not undefined

**Numeric**
- `.to.be.above(n)` / `.to.be.greaterThan(n)`
- `.to.be.below(n)` / `.to.be.lessThan(n)`
- `.to.be.at.least(n)` / `.to.be.at.most(n)`
- `.to.be.within(lo, hi)`
- `.to.be.closeTo(n, delta)`
- `.to.be.finite`

**String / array / object**
- `.to.include(val)` / `.to.contain(val)` — string or array membership
- `.to.have.length(n)` / `.to.have.lengthOf(n)`
- `.to.have.property(key)` / `.to.have.property(key, val)`
- `.to.have.deep.property(key, val)`
- `.to.have.own.property(key)`
- `.to.have.nested.property(path)`
- `.to.have.any.keys(...)` / `.to.have.all.keys(...)`
- `.to.have.members([...])` — array membership (unordered)
- `.to.have.ordered.members([...])`
- `.to.include.members([...])`
- `.to.satisfy(fn)` — custom predicate

**String specific**
- `.to.match(regexp)`
- `.to.have.string(str)` — substring
- `.to.start.with(str)` / `.to.end.with(str)`

**Object / array structure**
- `.to.be.empty` — zero-length string, array, or object
- `.to.be.an.instanceof(Constructor)`
- `.to.be.extensible` / `.to.be.sealed` / `.to.be.frozen`

**Function**
- `.to.throw()` / `.to.throw(Error)` / `.to.throw('message')`
- `.to.respondTo(method)`
- `.to.change(obj, prop)` / `.to.increase(obj, prop)` / `.to.decrease(obj, prop)`

**Negation**
- `.to.not.<any matcher>` — negates any of the above

**Language chains (no-ops for readability)**
- `.to`, `.be`, `.been`, `.is`, `.that`, `.which`, `.and`, `.has`, `.have`, `.with`, `.at`, `.of`, `.same`, `.but`, `.does`, `.still`

---

## 5. TypeScript type definitions

### 5.1 Add `@types/chai` as a dev dependency

```bash
yarn add -D @types/chai
```

### 5.2 Update `TEST_DEFS` in `rok-types.ts`

Replace the hand-rolled `ChaiAssertion` interface with a reference to the official Chai types. The `TEST_DEFS` string becomes:

```typescript
const TEST_DEFS = `
/// <reference types="chai" />

/** Register a named assertion block. Each block runs independently. */
declare function test(name: string, fn: () => void): void;

/** Full Chai expect — chain assertions with .to.equal(), .to.have.property(), .to.match(), etc. */
declare const expect: Chai.ExpectStatic;
`;
```

This gives Monaco full autocomplete and hover documentation for every Chai matcher.

> **Implementation note:** The `/// <reference types="chai" />` directive inside a Monaco `addExtraLib` string may not resolve `@types/chai` from the project's `node_modules`. If it doesn't, the fallback is to inline the content of `node_modules/@types/chai/index.d.ts` directly into the `TEST_DEFS` string at build time (or copy it as a local `.d.ts` file and read it as a static import). Verify this at implementation time before choosing the approach.

---

## 6. Snippets sidebar expansion

### 6.1 Common Tests — new entries

Expand the `common-tests` group with additional task-oriented templates:

| Label | Inserted code |
|---|---|
| Body is array | `test("Body is array", () => { expect(res.getBody()).to.be.an("array"); });` |
| Body is not empty | `test("Body is not empty", () => { expect(res.getBody()).to.not.be.empty; });` |
| Header equals value | `test("Header equals value", () => { expect(res.getHeader("content-type")).to.include("application/json"); });` |
| Body deep equals | `test("Body deep equals", () => { const body = res.getBody(); expect(body).to.deep.equal({ key: "value" }); });` |
| Property equals value | `test("Property equals value", () => { const body = res.getBody(); expect(body.key).to.equal("value"); });` |

### 6.2 `expect` API Reference — expanded entries

Replace the 7-item `expect` subgroup with a comprehensive set covering all major matchers:

**Equality**
- `.to.equal(value)`
- `.to.deep.equal(value)`
- `.to.eql(value)`

**Existence / truthiness**
- `.to.exist`
- `.to.be.ok`
- `.to.be.true`
- `.to.be.false`
- `.to.be.null`
- `.to.be.undefined`

**Type**
- `.to.be.a("type")`
- `.to.be.an.instanceof(Constructor)`

**Numeric**
- `.to.be.above(n)`
- `.to.be.below(n)`
- `.to.be.within(min, max)`
- `.to.be.closeTo(n, delta)`
- `.to.be.at.least(n)`
- `.to.be.at.most(n)`

**String / array / object**
- `.to.include("str")`
- `.to.have.length(n)`
- `.to.have.property("key")`
- `.to.have.property("key", value)`
- `.to.match(/regex/)`
- `.to.be.empty`
- `.to.have.members([...])`
- `.to.have.keys(...)`

**Negation**
- `.to.not.equal(value)`
- `.to.not.have.property("key")`
- `.to.not.include("str")`
- `.to.not.be.null`
- `.to.not.be.undefined`
- `.to.not.be.empty`

**Function**
- `.to.throw()`

---

## 7. Files changed

| File | Change |
|---|---|
| `crates/rocket-infra/src/scripting/bootstrap.js` | Replace hand-rolled `expect()` with Chai delegation + `toBe` patch |
| `src/components/editor/rok-types.ts` | Replace `ChaiAssertion` interface with `Chai.ExpectStatic` reference; expand snippets |
| `package.json` | Add `@types/chai` as devDependency |

No Rust changes. No IPC changes. No test result display changes.

---

## 8. Verification

```bash
yarn add -D @types/chai
yarn tsc --noEmit     # type-check — TEST_DEFS must reference Chai types cleanly
yarn check            # biome lint
cargo check -p rocket-infra   # bootstrap.js is include_str!, verify no compile errors
```

Manual:
1. Open any request → Scripts tab → Tests sub-tab.
2. Write `expect(res.getBody()).to.` and verify Monaco shows full Chai completions.
3. Write a test using `.to.deep.equal({})`, `.to.match(/regex/)`, `.to.not.be.empty` — verify no JS errors.
4. Run the request — verify pass/fail results display correctly.

---

## 9. Out of scope

| Feature | Notes |
|---|---|
| `to.have.status(code)` convenience | Dropped — use `expect(res.getStatus()).to.equal(code)` |
| Chai plugins (chai-http, chai-as-promised) | Not bundled; users can `require('chai')` and configure manually |
| Async assertions | Chai's `.eventually` requires chai-as-promised; out of scope |
