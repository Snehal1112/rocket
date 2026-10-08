# rok parity B, plan 05: snippets, docs and the final manual check

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the new async APIs discoverable in the script editor (snippets, no false error squiggles for top-level `await`), record the Safe-mode network exposure in the security spec, bring the spec and guidance docs in line with what was built, and run the final manual check of sub-project B.

**Architecture:** Snippet entries and a small pure helper live in `src/components/editor/rok-types.ts`; `MonacoWrapper.tsx` applies the helper to Monaco's JavaScript diagnostics options when a script editor mounts. The rest is documentation.

**Tech Stack:** TypeScript, React, Monaco 0.55, Vitest, Markdown.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md` (Risks and Testing sections). Index with rulings: `00-plan-index.md` (ruling 14, and the whole list for Task 2). Requires plans 01 to 04.

## Global Constraints

- Frontend rules: shadcn/ui primitives and `lucide-react` only (nothing new is rendered here), narrow Zustand selectors, Monaco for multi-line editors.
- `rok-types.ts` and `bootstrap.js` stay in sync (the sync test).
- Docs: short full sentences, no emojis. Project guidance goes in `.claude/` or the crate `CLAUDE.md` files; the root `CLAUDE.md` stays small.
- Comments are short full sentences ending in a period.

## Review Focus

- A script editor that shows `await rok.sendRequest(...)` at the top level: no red squiggle, while a real syntax error still shows one.
- The diagnostics change must not drop codes another editor already ignores (the helper merges, it does not replace).
- Inserting the "Fetch a token" template into a pre-request script produces code that runs as is.
- The security spec addendum states plainly that Safe mode now allows network calls; a reader of the trust model must not think Safe mode is offline.
- The spec's corrections match the rulings in `00-plan-index.md` word for word where they name functions.

---

### Task 1: Snippets and top-level `await` in the editor

**Files:**
- Modify: `src/components/editor/rok-types.ts` (`ROK_SNIPPETS`, `POST_RESPONSE_SNIPPETS`, `PRE_REQUEST_SNIPPETS`, new exports)
- Modify: `src/components/editor/MonacoWrapper.tsx` (the phase effect)
- Test: `src/components/editor/__tests__/rok-types.test.ts`, `src/components/editor/__tests__/MonacoWrapper.test.tsx` (the `monaco-editor` mock)

**Interfaces:**
- Consumes: the typings from plans 01, 03 and 04 (`sendRequest`, `sleep`, `runRequest`).
- Produces: `export const SCRIPT_TOP_LEVEL_DIAGNOSTIC_CODES: number[]` and `export function withScriptTopLevelAllowed<T extends { diagnosticCodesToIgnore?: number[] }>(options: T): T` in `rok-types.ts`.

- [ ] **Step 1: Write the failing tests**

In `src/components/editor/__tests__/rok-types.test.ts`, add `SCRIPT_TOP_LEVEL_DIAGNOSTIC_CODES` and `withScriptTopLevelAllowed` to the existing import from `'../rok-types'`, and add at the end of the file:

```ts
describe('async host call snippets and typings', () => {
  it.each([
    ['tests', ROK_SNIPPETS],
    ['pre-request', PRE_REQUEST_SNIPPETS],
    ['post-response', POST_RESPONSE_SNIPPETS],
  ] as const)('%s list offers sendRequest, runRequest and sleep', (_phase, groups) => {
    const labels = rokItemLabels(groups);
    expect(labels).toContain('await rok.sendRequest({ url })');
    expect(labels).toContain('await rok.runRequest("folder/request")');
    expect(labels).toContain('await rok.sleep(ms)');
  });

  it('pre-request common patterns include the token fetch template', () => {
    const common = PRE_REQUEST_SNIPPETS.find((g) => g.id === 'common-patterns');
    const item = common?.items?.find((i) => i.label === 'Fetch a token before the request');
    expect(item?.kind).toBe('template');
    expect(item?.code).toContain('await rok.sendRequest(');
  });

  it.each([
    'pre-request',
    'post-response',
    'tests',
  ] as const)('%s typings declare the async calls', (phase) => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE(phase);
    expect(defs).toContain('sendRequest(options: RokSendRequestOptions): Promise<RokResponse>;');
    expect(defs).toContain('runRequest(path: string)');
    expect(defs).toContain('sleep(ms: number): Promise<void>;');
  });
});

describe('withScriptTopLevelAllowed', () => {
  it('adds the top-level await and return codes', () => {
    const out = withScriptTopLevelAllowed({
      noSemanticValidation: false,
      diagnosticCodesToIgnore: [] as number[],
    });
    for (const code of SCRIPT_TOP_LEVEL_DIAGNOSTIC_CODES) {
      expect(out.diagnosticCodesToIgnore).toContain(code);
    }
    expect(out.noSemanticValidation).toBe(false);
  });

  it('keeps codes that were already ignored and adds no duplicates', () => {
    const once = withScriptTopLevelAllowed({ diagnosticCodesToIgnore: [2304, 1108] });
    const twice = withScriptTopLevelAllowed(once);
    expect(twice.diagnosticCodesToIgnore).toContain(2304);
    expect(twice.diagnosticCodesToIgnore?.filter((c) => c === 1108)).toHaveLength(1);
    expect(twice.diagnosticCodesToIgnore).toHaveLength(once.diagnosticCodesToIgnore?.length ?? 0);
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test rok-types`
Expected: FAIL. The import of `withScriptTopLevelAllowed` is undefined, and the snippet labels are missing.

- [ ] **Step 3: Add the snippets and the helper**

In `src/components/editor/rok-types.ts`:

1. In each of the three `rok` sub-groups (`id: 'rok'` inside `ROK_SNIPPETS`, `POST_RESPONSE_SNIPPETS` and `PRE_REQUEST_SNIPPETS`), add these items after the `rok.runner.setNextRequest("name")` item:

```ts
          {
            label: 'await rok.sendRequest({ url })',
            kind: 'expression',
            code: 'await rok.sendRequest({ method: "GET", url: "https://example.com" })',
          },
          {
            label: 'await rok.runRequest("folder/request")',
            kind: 'expression',
            code: 'await rok.runRequest("folder/request")',
          },
          { label: 'await rok.sleep(ms)', kind: 'expression', code: 'await rok.sleep(1000)' },
```

2. In `PRE_REQUEST_SNIPPETS`, add to the `common-patterns` items, after the `Log request details` item:

```ts
      {
        label: 'Fetch a token before the request',
        kind: 'template',
        code: `const res = await rok.sendRequest({\n  method: "POST",\n  url: rok.interpolate("{{baseUrl}}/auth/token"),\n  data: { clientId: rok.getEnvVar("clientId") },\n});\nrok.setVar("token", res.data.access_token);\nreq.setHeader("Authorization", "Bearer " + rok.getVar("token"));`,
      },
```

3. Add above `/** Returns the Monaco extra-lib ...` (the `ROK_TYPE_DEFS_FOR_PHASE` doc comment):

```ts
/**
 * TypeScript diagnostics a script must not show. Scripts run as the body of an
 * async function, so top-level `return` (1108) and `await` (1308, 1375, 1378)
 * are valid there.
 */
export const SCRIPT_TOP_LEVEL_DIAGNOSTIC_CODES = [1108, 1308, 1375, 1378];

/** Returns the diagnostics options with the script top-level codes also ignored. */
export function withScriptTopLevelAllowed<T extends { diagnosticCodesToIgnore?: number[] }>(
  options: T,
): T {
  const codes = new Set([
    ...(options.diagnosticCodesToIgnore ?? []),
    ...SCRIPT_TOP_LEVEL_DIAGNOSTIC_CODES,
  ]);
  return { ...options, diagnosticCodesToIgnore: [...codes] };
}
```

- [ ] **Step 4: Apply the helper in the editor**

In `src/components/editor/MonacoWrapper.tsx`, change the import line `import { ROK_TYPE_DEFS_FOR_PHASE } from './rok-types';` to:

```ts
import { ROK_TYPE_DEFS_FOR_PHASE, withScriptTopLevelAllowed } from './rok-types';
```

and in the phase effect, add directly after `if (!phase) return;`:

```ts
    // Scripts run as async function bodies, so top-level await and return are valid.
    const jsDefaults = monacoNs.typescript.javascriptDefaults;
    jsDefaults.setDiagnosticsOptions(withScriptTopLevelAllowed(jsDefaults.getDiagnosticsOptions()));
```

The `monaco-editor` mock in `src/components/editor/__tests__/MonacoWrapper.test.tsx` has no diagnostics functions, so a rendered editor with a phase would throw. Change its `javascriptDefaults` object to:

```tsx
    javascriptDefaults: {
      addExtraLib: vi.fn(),
      setCompilerOptions: vi.fn(),
      getCompilerOptions: vi.fn(() => ({})),
      getDiagnosticsOptions: vi.fn(() => ({})),
      setDiagnosticsOptions: vi.fn(),
    },
```

- [ ] **Step 5: Run the checks**

Run: `yarn test rok-types && yarn test MonacoWrapper && yarn tsc --noEmit && yarn check`
Expected: all PASS. If `yarn check` reformats the long template string, run `yarn format` and re-run `yarn check`.

- [ ] **Step 6: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `src/components/editor/rok-types.ts`, `src/components/editor/MonacoWrapper.tsx`, `src/components/editor/__tests__/rok-types.test.ts`, `src/components/editor/__tests__/MonacoWrapper.test.tsx`.
Suggested subject: `feat(editor): add async rok snippets and allow top-level await`.

---

### Task 2: Docs and the final manual check

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `docs/superpowers/specs/2026-10-07-js-script-security-design.md` (append an addendum)
- Modify: `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md` (append implementation notes)
- Modify: `.claude/rok-api-parity-notes.md` (the B row and "Notes for B")
- Modify: `crates/rocket-infra/CLAUDE.md` (a "Scripting engine" paragraph)
- Modify: `crates/rocket-app/CLAUDE.md` (a "Script host" bullet)

**Interfaces:**
- Consumes: everything in plans 01 to 04.
- Produces: documentation only.

- [ ] **Step 1: Add the security addendum**

Append to the end of `docs/superpowers/specs/2026-10-07-js-script-security-design.md`:

```markdown
## Addendum (2026-10-08): network access from Safe mode

rok parity B adds `rok.sendRequest` and `rok.runRequest`, and both work in Safe mode, like Bruno (user ruling). This widens the threat model above. Safe mode is where untrusted collections run, so a script from a cloned or imported collection can now send environment values, RocketVault values and response data to any host it names. The trust gate does not stop this.

- Script requests reuse the calling request's TLS, proxy and client-certificate settings. A client certificate is only presented when its domain matches the host the script picked, as for any send.
- Console lines and rejection messages mask secret values. The requests themselves carry the real values.
- Mitigations to consider later: show script-originated requests in History with a badge, and have the static scanner flag `rok.sendRequest`, `rok.runRequest` and, with part C, `rok.cookies.jar`.
```

- [ ] **Step 2: Record what was built against the B spec**

Append to the end of `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`:

```markdown
## Implementation notes (2026-10-08)

The plans in `docs/superpowers/plans/rok-parity-b/` corrected these points after reading the code. The full rulings are in `00-plan-index.md`.

- The script thread was a `spawn_blocking` thread on the caller's runtime with no event loop. It now builds its own current-thread Tokio runtime, because deno_core needs one for async ops.
- No Tokio `Handle` crosses to the script thread. Host calls travel over a channel to the task that called the engine, which serves them with a borrowed `&dyn ScriptHost`.
- No `Arc`/`Weak` handle was needed: `ExecutionScriptHost<'a>` borrows `RequestExecutionService`, and nested runs reuse the same stateless engine.
- The budget counts busy time (time running code) rather than subtracting await time, so an unawaited request cannot hide a busy loop. The watchdog stops a waiting run with an abort flag and running code with `terminate_execution`.
- Wrapping scripts as async functions makes top-level `var` and function declarations local, runs promise callbacks that used to be dropped, and turns an unhandled rejection into the script error. Error text keeps its `Uncaught` prefix.
- `test()` awaits async bodies. 4xx and 5xx responses resolve. `runRequest` paths are request file paths relative to the collection root, without extension. GraphQL items run; WebSocket and gRPC items are skipped.
- Known limits: env, collection and global writes a script makes before `runRequest` are not visible to the nested run, and later phases of the outer request do not see env values the nested run wrote (as with `setEnvVar` today).
```

- [ ] **Step 3: Update the series notes**

In `.claude/rok-api-parity-notes.md`, replace the B row of the status table with:

```markdown
| B | Async host calls: `sendRequest`, `runRequest`, `sleep` (`req.onFail` stays a no-op) | Implemented from `docs/superpowers/plans/rok-parity-b/` (index plus 5 plans). Follow-ups: History badge for script requests, scanner flags for `sendRequest`/`runRequest`, nested runs seeing the caller's unsaved env writes, refreshing `PhaseState.var_ctx.env` after env writes. |
```

and replace the bullets under `## Notes for B` with:

```markdown
- Engine model: scripts run as `(async function () { ... }).call(globalThis)` on a per-script current-thread Tokio runtime inside `spawn_blocking`. Host calls go over a channel to `run_script_bounded`, which serves them with the borrowed `&dyn ScriptHost`.
- Budget: `ScriptLimits` (5 s busy time, 5 min ceiling, 60 s sleep cap) in `crates/rocket-infra/src/scripting/budget.rs`. `DenoScriptEngine::with_limits` injects short limits for tests.
- `ScriptHost` (`crates/rocket-scripting/src/host.rs`) has defaulted methods, so part C adds cookie methods the same way `run_request` was added.
- `ExecutionScriptHost` (`crates/rocket-app/src/execution_service/script_host.rs`) borrows the service. `runRequest` lookup and the recursion guard are in `execution_service/run_request.rs`.
```

- [ ] **Step 4: Update the crate guidance**

In `crates/rocket-infra/CLAUDE.md`, insert this paragraph directly above the `## Testing` heading (leave the gRPC paragraph above it unchanged):

```markdown
**Scripting engine (`scripting/`).** `DenoScriptEngine` runs each script on a `spawn_blocking` thread that builds its own current-thread Tokio runtime (deno_core's async ops need one) and wraps the code as `(async function () { ... }).call(globalThis)`, so top-level `await` and `return` work. The event loop runs until the script's promise settles and then until no work is left. Host calls (`rok.sendRequest`, `rok.runRequest`) are async ops that send a `HostCall` over a channel (`host_bridge.rs`); `run_script_bounded` in `engine.rs` serves them on the caller's task with the borrowed `&dyn ScriptHost`, and without a host they reject with "is not available here". `budget.rs` holds `ScriptLimits` (5 s busy time, 5-minute ceiling, 60 s `rok.sleep` cap) and the `BudgetClock`: busy time is time spent running code, so waiting does not count. The watchdog stops a waiting run with the clock's abort flag and running code with `terminate_execution`. Console lines and errors from host calls are masked with `ops::redact`.
```

In `crates/rocket-app/CLAUDE.md`, add this bullet directly after the `- **Folder script chain.** ...` bullet:

```markdown
- **Script host.** Every request-phase script run gets an `ExecutionScriptHost` (`execution_service/script_host.rs`) through `ScriptEngine::execute_with_host`. It borrows the service, so no `Arc` or second engine exists. `rok.sendRequest` builds an `HttpRequest` with the calling request's resolved options (TLS, redirects, cookie jar, client certificates, vault certificates fetched first) and enforces the script's timeout itself. `rok.runRequest` finds the target with `execution_service/run_request.rs` (file path relative to the collection root, no extension; WebSocket and gRPC are skipped), checks `PhaseState.run_chain` (no revisits, at most 5 nested levels) and runs `execute_nested`, which drives the same phases. Nested runtime changes and re-read env, global and collection scopes flow back to the calling script. `apply_actions` and Flow transform and condition nodes run without a host.
```

- [ ] **Step 5: Run the full verification**

Run:

```bash
cargo check -j4
cargo test -j4 -p rocket-scripting
cargo test -j4 -p rocket-infra scripting
cargo test -j4 -p rocket-app execution_service
cargo test -j4 -p rocket-app collection_runner_service
cargo test -j4 -p rocket-app flow_execution_service
yarn tsc --noEmit
yarn check
yarn test rok-types
```

Expected: all PASS.

- [ ] **Step 6: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `docs/superpowers/specs/2026-10-07-js-script-security-design.md`, `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`, `.claude/rok-api-parity-notes.md`, `crates/rocket-infra/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`.
Suggested subject: `docs: record rok parity B design notes and Safe-mode network access`.

## Manual check (real app, after all five plans)

Run `yarn tauri dev` and work through this list. Hand it to the user if the session cannot drive the app.

1. Pre-request script: `const r = await rok.sendRequest({ url: 'https://httpbin.org/json' }); console.log(r.status, r.data.slideshow.title)`. The Console shows the `rok.sendRequest GET ... -> 200` line and the values. No red squiggle under `await` in the editor.
2. Same script with `timeout: 1`: the rejection reads `rok.sendRequest: timed out after 1 ms`.
3. A URL holding a secret environment variable: the Console line masks it.
4. Callback form: `rok.sendRequest({ url: 'https://httpbin.org/status/404' }, (err, res) => console.log(err, res.status))` logs `null 404`.
5. `await rok.sleep(2000); console.log('ok')` finishes after about two seconds without a timeout error; `while (true) {}` still stops after about five seconds with a "timed out" error.
6. `await rok.runRequest('<folder>/<request>')` on a saved request: History shows both requests, and a runtime variable set by the nested request's post-response script is readable right after the call and in the tests script.
7. A Flow transform node that calls `rok.sendRequest` fails with "is not available here".
8. The tests tab: `test('async', async () => { const r = await rok.sendRequest({ url: 'https://httpbin.org/status/500' }); expect(r.status).to.equal(200); })` is reported as failed.

---

## Next plan to execute

This is the last plan of sub-project B. When Task 2 is complete, its checks pass, the manual check above is done or handed to the user, and the ledger (`.superpowers/sdd/rok-parity-b-05-snippets-docs-and-manual-check/progress.md`) shows "Task 2: complete", the executing Claude must:

1. Run a final whole-branch review of plans 01 to 05 together (see `.claude/rules/harness.md`).
2. Move on to **sub-project C (cookies)** without asking. Its spec is committed: `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`. There is no plan for it yet, so first invoke the `superpowers:writing-plans` skill and write the plan files (at most 3 tasks each, plus an index) under `docs/superpowers/plans/rok-parity-c/`. C extends `ScriptHost` with defaulted cookie methods, serves them in `ExecutionScriptHost`, and must confirm that `CookieService` and `RepoCookieStore` share one lock or one repo instance (see the C spec's Risks).
