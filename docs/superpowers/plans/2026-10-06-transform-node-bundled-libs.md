# Transform Node Bundled Libraries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a flow Transform node script `require()` the bundled libraries (existing ones plus lodash), with working editor typings.

**Architecture:** Transform scripts already run through the same deno_core engine and `bootstrap.js` as request scripts, so the global `require()` should already resolve the fixed bundled set (`op_require_module` in `crates/rocket-infra/src/scripting/engine.rs`). The work is: (1) prove it with a real-engine test, (2) vendor lodash into the bundled set, (3) declare `require` in the Transform editor's Monaco typings so it is not flagged as an error, (4) document, (5) security-review. Safe mode is unchanged: no npm install, no local-file require (explicit non-goal in `docs/superpowers/specs/2026-09-21-sandbox-developer-mode-design.md:164`).

**Tech Stack:** Rust (deno_core 0.400, `rocket-infra`), React/TypeScript, Monaco, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-21-sandbox-developer-mode-design.md` (non-goals), `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md:200-212` (bundled library list), `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md`.

## Global Constraints

- Safe sandbox mode stays unchanged; no new ops, no filesystem or network access from `require`.
- Unknown module names must still throw `Module not found: <name>`.
- Vendored code must keep its license header. Lodash is MIT; copy from `node_modules/lodash/lodash.min.js` (v4.18.1, transitive dependency, so do not add it to `package.json`).
- Rust: no `unwrap()` in production paths. Commits use conventional-commit format.
- UI: shadcn/ui primitives only, lucide-react icons, Monaco for multi-line editors.

## Review Focus

- `require('lodash')` in Safe mode: lodash uses `Function('return this')()` to find the global; must not break under the sandbox or expose `Deno`/`__bootstrap`.
- Script startup cost: lodash is ~70 KB min; confirm a Transform run with `require('lodash')` stays well under the 5 s `SCRIPT_TIMEOUT`.
- `require` of a bundled lib twice in one script returns a working module each time (no caching bug).
- Transform script that returns `undefined` after a `require` still reports "script returned no value".
- `require('fs')` / `require('./x')` in Transform fails with `Module not found`, not a crash.

---

### Task 1: Prove Transform can `require()` bundled libs (Sonnet)

**Model:** Sonnet. Needs judgement about the existing test harness.

**Files:**
- Modify (tests only): `crates/rocket-infra/src/scripting/engine.rs` (tests module, next to `require_uuid_v4` ~line 772)

**Interfaces:**
- Consumes: `DenoScriptEngine::new()`, `minimal_ctx(script: &str)`, `engine.execute(ctx)` returning a result with `.error: Option<String>` and `.runtime_vars`.
- Produces: tests `transform_style_body_can_require_uuid`, `transform_style_body_unknown_module_errors` that pin the behavior later tasks rely on.

The Transform wrapper (`flow_script` in `crates/rocket-app/src/flow_execution_service.rs:97`) runs the user source inside `new Function('response', src)` and the engine wraps it in `rok.setVar('__jsonq_result__', ...)`. These tests reproduce that shape against the real engine.

- [ ] **Step 1: Write the tests**

```rust
    #[tokio::test]
    async fn transform_style_body_can_require_uuid() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const out = (function () {
                const fn = new Function('response', "const { v4 } = require('uuid'); return v4();");
                return fn({ body: null });
            })();
            rok.setVar('out', out);
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        let id = result.runtime_vars.get("out").expect("out").as_str().expect("string");
        assert_eq!(id.len(), 36);
    }

    #[tokio::test]
    async fn transform_style_body_unknown_module_errors() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const fn = new Function('response', "return require('fs');");
            fn({ body: null });
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result
            .error
            .as_ref()
            .expect("error expected")
            .contains("Module not found"));
    }
```

- [ ] **Step 2: Run**

Run: `cargo test -p rocket-infra transform_style_body`
Expected: PASS. If `require` is NOT visible inside `new Function` bodies, this FAILS: stop and fix `bootstrap.js` (`globalThis.require` at ~line 341) before continuing; report the finding.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/scripting/engine.rs
git commit -m "test(scripting): pin require() inside Transform-style function bodies"
```

---

### Task 2: Vendor lodash into the bundled set (Haiku)

**Model:** Haiku. Mechanical copy plus one match arm and tests; exact code below.

**Files:**
- Create: `crates/rocket-infra/src/scripting/modules/lodash.js`
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (`op_require_module` match, ~line 136; tests module)

**Interfaces:**
- Consumes: `op_require_module(name) -> String` match arms.
- Produces: `require('lodash')` returning the lodash function object (`_`), with `_.groupBy`, `_.get`, etc.

- [ ] **Step 1: Write the failing test** (add in the tests module)

```rust
    #[tokio::test]
    async fn require_lodash_group_by_and_get() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            r#"
            const _ = require('lodash');
            const g = _.groupBy([{t:'a'},{t:'b'},{t:'a'}], 't');
            rok.setVar('n', String(g.a.length));
            rok.setVar('deep', String(_.get({a:{b:[7]}}, 'a.b[0]')));
        "#,
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("n").expect("n"), "2");
        assert_eq!(result.runtime_vars.get("deep").expect("deep"), "7");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rocket-infra require_lodash`
Expected: FAIL with `Module not found: lodash`.

- [ ] **Step 3: Vendor and register**

```bash
cp node_modules/lodash/lodash.min.js crates/rocket-infra/src/scripting/modules/lodash.js
```

Add to the match in `op_require_module`, before `_ => String::new()`:

```rust
        "lodash" => include_str!("modules/lodash.js").to_string(),
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p rocket-infra require_lodash && cargo test -p rocket-infra scripting::`
Expected: PASS, including `unknown_require_returns_error`.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/scripting/modules/lodash.js crates/rocket-infra/src/scripting/engine.rs
git commit -m "feat(scripting): bundle lodash for require()"
```

---

### Task 3: Declare `require` in Transform editor typings (Sonnet)

**Model:** Sonnet. Frontend, with Monaco typing behavior to get right.

**Files:**
- Modify: `src/components/flow/wire-script-types.ts`
- Test: `src/components/flow/properties/__tests__/TransformNodeEditor.test.tsx` (or a new `src/components/flow/__tests__/wire-script-types.test.ts`)

**Interfaces:**
- Consumes: `WIRE_SCRIPT_TYPES: string`, `WIRE_SCRIPT_TYPES_PATH`, already passed to Monaco as an extra lib by `TransformNodeEditor.tsx`.
- Produces: `WIRE_SCRIPT_TYPES` that also declares `require`.

- [ ] **Step 1: Write the failing test** (new file `src/components/flow/__tests__/wire-script-types.test.ts`)

```ts
import { describe, expect, it } from 'vitest';
import { WIRE_SCRIPT_TYPES } from '../wire-script-types';

describe('WIRE_SCRIPT_TYPES', () => {
  it('declares require() and every bundled module name', () => {
    expect(WIRE_SCRIPT_TYPES).toContain('declare function require');
    for (const name of ['lodash', 'uuid', 'moment', 'crypto-js', 'nanoid', 'chai']) {
      expect(WIRE_SCRIPT_TYPES).toContain(`'${name}'`);
    }
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test wire-script-types`
Expected: FAIL (no `require` declaration).

- [ ] **Step 3: Implement** — append inside the template string in `wire-script-types.ts`, after the `response` declaration:

```ts
/** Modules bundled with the script sandbox. Keep in sync with op_require_module in rocket-infra. */
declare function require(name: 'lodash'): any;
declare function require(name: 'uuid'): any;
declare function require(name: 'moment'): any;
declare function require(name: 'crypto-js'): any;
declare function require(name: 'nanoid'): any;
declare function require(name: 'chai'): any;
declare function require(name: 'jsonwebtoken'): any;
declare function require(name: 'jsrsasign'): any;
declare function require(name: 'tv4'): any;
declare function require(name: 'atob' | 'btoa'): any;
```

(`axios` is intentionally omitted: it loads but cannot make requests.)

- [ ] **Step 4: Verify**

Run: `yarn test wire-script-types && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/flow/wire-script-types.ts src/components/flow/__tests__/wire-script-types.test.ts
git commit -m "feat(flow): type require() for bundled libs in Transform editor"
```

---

### Task 4: Update docs (Haiku)

**Model:** Haiku.

**Files:**
- Modify: `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` (bundled library list, ~lines 200-212: add lodash)
- Modify: `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (add a short "Libraries" note: Transform can `require()` the bundled set; no npm/local files)

- [ ] **Step 1:** Add `lodash` to the library list and the Transform note, in the existing style of each file.
- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs
git commit -m "docs(scripting): list lodash and Transform require() support"
```

---

### Task 5: Security and whole-branch review (Opus)

**Model:** Opus. Judgement-heavy; runs last.

**Files:** none modified unless findings are fixed.

- [ ] **Step 1:** Review `git diff main...HEAD` against the Review Focus list: confirm lodash cannot reach `Deno`/`__bootstrap`, startup time is acceptable (time a Transform run with `require('lodash')`), and Safe mode ops are unchanged.
- [ ] **Step 2: Full verification**

Run: `cargo check && cargo test -p rocket-infra && cargo test -p rocket-app transform && yarn tsc --noEmit && yarn check && yarn test`
Expected: all PASS; report any failure verbatim.

---

## Model assignment summary

| Task | Model | Why |
|---|---|---|
| 1 Characterization tests | Sonnet | Must read the existing harness; may uncover a real bug |
| 2 Vendor lodash | Haiku | Copy file, one match arm, exact test given |
| 3 Editor typings | Sonnet | Frontend plus Monaco behavior |
| 4 Docs | Haiku | Small text edits |
| 5 Review | Opus | Security judgement, whole-branch view |

Tasks 1 and 3 are independent and can run in parallel. Task 2 follows Task 1. Task 4 follows Tasks 2 and 3. Task 5 is last.

## Self-Review

- Spec coverage: require in Transform (T1), new lib (T2), editor typing (T3), docs (T4), security (T5). npm and local-file require are out of scope by decision.
- Placeholder scan: none.
- Review Focus coverage: lodash global lookup and startup time covered in T2 test and T5; unknown module in T1; undefined-return behavior is already tested by `transform_script_reports_a_script_error` in `flow_execution_service.rs` and is unchanged.
