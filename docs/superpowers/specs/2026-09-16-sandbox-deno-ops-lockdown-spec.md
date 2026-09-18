# Spec: Hide `Deno`/`Deno.core.ops` From User Scripts

**Status:** Draft
**Severity:** Medium
**Roadmap:** [2026-09-16-scripting-security-roadmap.md](../plans/2026-09-16-scripting-security-roadmap.md), item 2
**Related:** independent of items 1/3/4/5/6 — can be implemented in any order relative to them.

## 1. Problem

`bootstrap.js` (`crates/rocket-infra/src/scripting/bootstrap.js`) implements every intended global
(`console`, `rok`, `req`, `res`, `require`, `test`, `expect`) by calling `Deno.core.ops.op_*`
directly (e.g. lines 12-14, 20-36, 140, 159-164). Nothing ever deletes or freezes
`globalThis.Deno` after setup. The user's script executes in the *same* `JsRuntime`/global scope
right after bootstrap (`crates/rocket-infra/src/scripting/engine.rs:164-173`,
`execute_script("<bootstrap>", ...)` then `execute_script("<user>", code)`).

Consequence: user script code can call `Deno.core.ops.op_print(...)` or
`Deno.core.ops.op_panic(...)` directly — deno_core's own built-in ops
(`ops_builtin.rs` in the `deno_core` crate, unconditionally registered by any `JsRuntime`
regardless of what extensions the embedder adds):

- `op_print` writes straight to the host process's stdout/stderr, unmediated and not captured into
  `ConsoleEntry`/`ScriptOutputState` — bypasses the intended console surface entirely.
- `op_panic` calls Rust `panic!("JS PANIC: {message}")` with an attacker-controlled string —
  currently caught by `tokio::task::spawn_blocking`'s `JoinHandle` and mapped to a
  `DomainError::Internal` (`engine.rs:32-34`), so it doesn't crash the whole app, but it is an
  unintended, undocumented capability that contradicts the doc comment at `engine.rs:9-13`
  ("No Deno standard library... only the rok, req, res, console, test, expect, and require
  globals").

This does **not** grant filesystem/network/process access — no such extensions are ever
registered (confirmed: `extension!(rocket_scripting_ext, ops = [...])` at `engine.rs:74-135` lists
only rocket's own ops) — so this is a containment/hygiene gap, not a sandbox breach. It's still a
real bug: the sandbox's stated contract is violated, and it gives a malicious script an
unnecessary DoS primitive against its own execution (and an unmediated stdout-write side channel).

## 2. Goal

After `bootstrap.js` finishes setup, `globalThis.Deno` must be completely unreachable from user
script code — `typeof Deno === 'undefined'` inside the user script — while every intended global
(`console`, `rok`, `req`, `res`, `require`, `test`, `expect`) continues to work exactly as before.

## 3. The subtlety that makes a naive fix wrong

A naive fix — appending `delete globalThis.Deno;` as the last line of `bootstrap.js` — **breaks
the entire scripting engine**. Every wrapper function currently defined (e.g.
`log: (...args) => Deno.core.ops.op_console_log(...)`, `bootstrap.js:12`) looks up the identifier
`Deno` from the global scope **at call time**, not at definition time — arrow functions close over
their lexical scope, and `Deno` is resolved as a global property lookup each time the function
runs. If `globalThis.Deno` is deleted before the user script runs, then the very first time the
user script calls `console.log(...)` (or `rok.getVar`, `req.getUrl`, anything), the wrapper's body
tries to evaluate `Deno.core.ops.op_console_log` and gets `ReferenceError: Deno is not defined` —
every rok/req/res/console/test call in every script would start throwing immediately.

## 4. Correct design

Capture a **private, non-global** reference to `Deno.core.ops` once, at the very top of
`bootstrap.js`, before any wrapper is defined — then rewrite every wrapper to call through that
private reference instead of the global `Deno`. Only after all wrappers are defined and assigned
onto `globalThis`, delete `globalThis.Deno`.

Wrap the whole file in an IIFE so the captured reference is a closure variable, not itself a new
global:

```js
"use strict";

(function () {
  const __ops = Deno.core.ops; // captured once, before Deno is removed from globalThis

  // ── console ──────────────────────────────────────────────────────────────
  function _fmt(v) { /* unchanged */ }
  const console = {
    log:   (...args) => __ops.op_console_log(args.map(_fmt).join(" ")),
    warn:  (...args) => __ops.op_console_warn(args.map(_fmt).join(" ")),
    error: (...args) => __ops.op_console_error(args.map(_fmt).join(" ")),
  };
  globalThis.console = console;

  // ── rok / req / res ──────────────────────────────────────────────────────
  // every `Deno.core.ops.op_X` reference in the current file becomes `__ops.op_X`
  // (mechanical rename — see implementation plan for the full list of ~40 call sites)

  // ── require() module loader ─────────────────────────────────────────────
  globalThis.require = function (name) {
    const src = __ops.op_require_module(name);
    if (!src) throw new Error(`Module not found: ${name}`);
    const mod = { exports: {} };
    const fn = new Function("module", "exports", "require", src);
    fn(mod, mod.exports, globalThis.require);
    return mod.exports;
  };

  // ── test()/expect(), rok.test/rok.expect aliases ────────────────────────
  // unchanged except op_test_run/op_test_pass/op_test_fail → __ops.op_test_*

  // Everything is wired up on globalThis now. Remove the raw Deno global so
  // user script code (which runs after this IIFE returns) cannot reach any
  // deno_core built-in op (op_print, op_panic, etc.) directly.
  delete globalThis.Deno;
})();
```

Key correctness points for the implementation plan:

- `new Function("module", "exports", "require", src)` (the `require()` module loader, currently
  line 143) creates a function whose only lexical parent is the **global** scope — it does *not*
  capture the enclosing IIFE's `__ops` closure variable, and does not need to: vendored modules
  (`crypto-js.js`, `jsrsasign.js`, etc.) were already confirmed in the prior audit to never
  reference `Deno` directly, only `globalThis.crypto`/`navigator`/`btoa`/`atob`, all of which
  remain on `globalThis` untouched by this change.
- The `navigator`/`atob`/`btoa` polyfills (bootstrap.js:83-136) don't touch `Deno` at all — leave
  them exactly where they are relative to the IIFE (inside or after, doesn't matter — they don't
  need `__ops`).
- `delete globalThis.Deno` must be the **last** statement in the IIFE, after every
  `globalThis.X = ...` assignment. Getting the order wrong (e.g. deleting before `require('chai')`
  is called to build `expect`) breaks `require()` since it internally calls `__ops.op_require_module`
  — as long as `require` itself uses `__ops` (not `Deno` directly) this is fine regardless of
  order, but keep the deletion last for clarity and to avoid future foot-guns as the file evolves.
- This closure-capture approach is airtight against `eval`/`Function`-based resurrection attempts:
  once `Deno` is deleted from `globalThis`, the only remaining reference to the ops object is the
  `__ops` `const` inside the IIFE's closure, which user script code — running in a completely
  separate `execute_script` call, outside that closure — has no syntactic way to reach. `eval("Deno")`
  or `Function("return Deno")()` from user code will `ReferenceError` exactly like any other
  deleted global.

## 5. Non-goals

- Not sandboxing further than "remove the accidental extra surface." The intended `rok`/`req`/`res`
  ops surface is unchanged and was already confirmed correctly scoped by phase guards in the prior
  audit (`ops/req.rs` `guard_before_request`, `ops/res.rs` `get_response`) — this spec does not
  revisit that.
- Not adding a CSP-style allowlist mechanism for future ops. If a new built-in op needs exposing
  later, it's added to the `__ops`-based wrappers the same way as today's ops.

## 6. Acceptance criteria (drives the plan's regression tests)

1. A user script that runs `globalThis.__typeofDeno = typeof Deno;` and reports it back (e.g. via
   `rok.setVar`) observes `"undefined"`.
2. A user script that runs `Deno.core.ops.op_print("pwned")` throws a `ReferenceError` (caught as
   a script error, surfaced via `ScriptResult.error` exactly like any other uncaught exception
   today — no new error-handling path needed, this is just the existing "script threw" path now
   correctly reached for this case instead of silently succeeding).
3. Full existing behavior is preserved: `console.log/warn/error`, `rok.getVar/setVar/getEnvVar/
   setEnvVar/hasEnvVar/deleteEnvVar/getEnvName/getCollectionVar/setCollectionVar/getGlobalEnvVar/
   setGlobalEnvVar/interpolate/runner.setNextRequest/runner.skipRequest`, `req.*` (all read and
   write ops), `res.*`, `require('chai'|'crypto-js'|'jsonwebtoken'|'jsrsasign'|'uuid'|'moment'|
   'nanoid'|'tv4'|'axios'|'atob'|'btoa')`, `test()`/`expect()`/`rok.test`/`rok.expect` all continue
   to behave identically to before this change.
4. All existing tests in `crates/rocket-infra/src/scripting/engine.rs`'s `#[cfg(test)] mod tests`
   block continue to pass unmodified (they exercise the wrapper surface end-to-end already, so
   they're the primary regression net for point 3).
5. `cargo test -p rocket-infra` passes.
