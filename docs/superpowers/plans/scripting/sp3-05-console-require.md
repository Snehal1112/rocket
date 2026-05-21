# SP3-05 — `rocket-infra`: Console Ops + Inbuilt `require()` Bundles

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `console.log/warn/error` capture ops and embed all nine inbuilt `require()` libraries (chai, crypto-js, jsonwebtoken, uuid, moment, axios, atob/btoa, nanoid, tv4) as `include_str!()` UMD bundles compiled into the binary.

**Architecture:** Console ops write to `ScriptOutputState.console_entries`. The `op_require_module` op (stub in SP3-02) is replaced with a `match` over the nine library names, returning their pre-bundled UMD source. Libraries are stored under `crates/rocket-infra/src/scripting/modules/` as `.js` files and embedded at compile time with `include_str!()`.

**Tech Stack:** Rust, `deno_core::op2`, Node.js (one-time bundle generation)

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §3

**Depends on:** SP3-02 merged.

---

## Task 1: Console ops (`log`, `warn`, `error`)

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/console.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs`

- [ ] **Step 1: Replace console stub with full ops**

```rust
use deno_core::{op2, OpState};
use rocket_scripting::ConsoleLevel;
use crate::scripting::state::ScriptOutputState;

#[op2(fast)]
pub fn op_console_log(state: &mut OpState, #[string] msg: String) {
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Log, msg);
}

#[op2(fast)]
pub fn op_console_warn(state: &mut OpState, #[string] msg: String) {
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Warn, msg);
}

#[op2(fast)]
pub fn op_console_error(state: &mut OpState, #[string] msg: String) {
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Error, msg);
}
```

- [ ] **Step 2: Register all three console ops in `build_extension()` inside `engine.rs`**

Replace the single console stub:

```rust
// console ops
console::op_console_log(),
console::op_console_warn(),
console::op_console_error(),
```

- [ ] **Step 3: Verify compile**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
```

- [ ] **Step 4: Add console level tests**

Append to `#[cfg(test)]` in `engine.rs`:

```rust
#[tokio::test]
async fn console_warn_and_error_captured() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("console.warn('watch out'); console.error('bad thing')");
    let result = engine.execute(ctx).await.unwrap();
    assert_eq!(result.console_entries.len(), 2);
    assert_eq!(result.console_entries[0].level, rocket_scripting::ConsoleLevel::Warn);
    assert_eq!(result.console_entries[1].level, rocket_scripting::ConsoleLevel::Error);
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p rocket-infra scripting::engine 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/scripting/
git commit -m "feat(rocket-infra): console log/warn/error ops"
```

---

## Task 2: Bundle inbuilt JS libraries

**Files:**
- Create: `crates/rocket-infra/src/scripting/modules/` (directory + all `.js` files)

- [ ] **Step 1: Create the modules directory**

```bash
mkdir -p crates/rocket-infra/src/scripting/modules
```

- [ ] **Step 2: Download UMD bundles for each library**

Run these commands from the repo root. Each downloads the pre-built UMD (CommonJS-compatible) bundle — no build step needed:

```bash
cd crates/rocket-infra/src/scripting/modules

# chai — assertion library
curl -sL "https://cdn.jsdelivr.net/npm/chai@4/chai.js" -o chai.js

# crypto-js
curl -sL "https://cdn.jsdelivr.net/npm/crypto-js@4/crypto-js.js" -o crypto-js.js

# jsonwebtoken — use jsrsasign-based UMD compatible build
curl -sL "https://cdn.jsdelivr.net/npm/jsonwebtoken@9/build/jsonwebtoken.js" -o jsonwebtoken.js

# uuid
curl -sL "https://cdn.jsdelivr.net/npm/uuid@9/dist/umd/uuid.min.js" -o uuid.js

# moment
curl -sL "https://cdn.jsdelivr.net/npm/moment@2/moment.js" -o moment.js

# nanoid — UMD build
curl -sL "https://cdn.jsdelivr.net/npm/nanoid@3/nanoid.cjs.js" -o nanoid.js

# tv4 — JSON Schema v4 validator
curl -sL "https://cdn.jsdelivr.net/npm/tv4@1/tv4.js" -o tv4.js

# axios — UMD build (uses op_axios_request Rust op for actual HTTP)
curl -sL "https://cdn.jsdelivr.net/npm/axios@1/dist/axios.js" -o axios.js

# atob/btoa — already available as V8 globals; create an empty pass-through
echo 'module.exports = { atob: globalThis.atob, btoa: globalThis.btoa };' > atob-btoa.js

cd -
```

> **Note to subagent:** If any URL returns a 404, find the correct CDN path for that package version. The key requirement is that each file is a UMD build (starts with `(function(root, factory) {` or similar CommonJS pattern). Verify each file starts with a recognisable UMD wrapper before proceeding.

- [ ] **Step 3: Verify all files exist and are non-empty**

```bash
ls -la crates/rocket-infra/src/scripting/modules/
wc -l crates/rocket-infra/src/scripting/modules/*.js
```

Expected: 9 files, all > 100 lines except `atob-btoa.js`.

- [ ] **Step 4: Commit bundles**

```bash
git add crates/rocket-infra/src/scripting/modules/
git commit -m "feat(rocket-infra): add inbuilt JS library UMD bundles"
```

---

## Task 3: Wire `op_require_module` to return bundled sources

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs`

- [ ] **Step 1: Replace the `op_require_module` stub in `engine.rs`**

Find the existing `fn op_require_module` and replace it entirely:

```rust
#[op2]
#[string]
fn op_require_module(#[string] name: String) -> String {
    match name.as_str() {
        "chai"         => include_str!("modules/chai.js").to_string(),
        "crypto-js"    => include_str!("modules/crypto-js.js").to_string(),
        "jsonwebtoken" => include_str!("modules/jsonwebtoken.js").to_string(),
        "uuid"         => include_str!("modules/uuid.js").to_string(),
        "moment"       => include_str!("modules/moment.js").to_string(),
        "nanoid"       => include_str!("modules/nanoid.js").to_string(),
        "tv4"          => include_str!("modules/tv4.js").to_string(),
        "axios"        => include_str!("modules/axios.js").to_string(),
        "atob" | "btoa" => include_str!("modules/atob-btoa.js").to_string(),
        _              => String::new(),
    }
}
```

- [ ] **Step 2: Compile check (this embeds all bundles at compile time)**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
```

Expected: zero errors. (This may take a moment — embedding ~1MB of JS at compile time.)

- [ ] **Step 3: Add `require()` integration tests**

Append to `#[cfg(test)]` in `engine.rs`:

```rust
#[tokio::test]
async fn require_chai_and_use_expect() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx(r#"
        const chai = require('chai');
        const expect = chai.expect;
        rok.setVar('result', 'pass');
        expect(1 + 1).to.equal(2);
    "#);
    let result = engine.execute(ctx).await.unwrap();
    assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
    assert_eq!(result.runtime_vars.get("result").unwrap(), "pass");
}

#[tokio::test]
async fn require_uuid() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx(r#"
        const { v4: uuidv4 } = require('uuid');
        const id = uuidv4();
        rok.setVar('id', id);
    "#);
    let result = engine.execute(ctx).await.unwrap();
    assert!(result.error.is_none());
    let id = result.runtime_vars.get("id").unwrap().as_str().unwrap();
    // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
    assert_eq!(id.len(), 36);
    assert_eq!(&id[14..15], "4");
}

#[tokio::test]
async fn unknown_require_returns_error() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("require('not-a-real-module')");
    let result = engine.execute(ctx).await.unwrap();
    assert!(result.error.is_some());
    assert!(result.error.as_ref().unwrap().contains("Module not found"));
}
```

- [ ] **Step 4: Run all scripting tests**

```bash
cargo test -p rocket-infra scripting 2>&1 | tail -30
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/scripting/engine.rs
git commit -m "feat(rocket-infra): wire op_require_module to embedded UMD bundles + tests"
```
