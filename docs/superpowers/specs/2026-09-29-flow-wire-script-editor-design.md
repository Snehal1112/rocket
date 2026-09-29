# Flow Wire Script Editor — Design

**Date:** 2026-09-29.
**Branch:** `worktree-flow-phase2-branching` (unmerged Flow Phase 2 + #26 work).
**Scope:** backend (rocket-app, rocket-shared) and frontend.

## 1. Problem and goal

A wire's "Value from source" expression is edited in a small popover:
- It is anchored to a zero-size `<span />` after the canvas (`FlowPane.tsx` ~391), so it opens at the bottom-left of the pane, never centred.
- It is a one-line `SingleLineEditor`, too small for real JavaScript.
- It only opens when a wire is drawn. An existing wire's expression cannot be edited; the user must delete and redraw it.
- The expression is wrapped as one JS expression, so statements (`const`, `;`, `return`) are a syntax error.
- `console.log` output is captured by the script engine (`ScriptResult.console_entries`) but `evaluate_var_expression` drops it, so the user can never see it.

**Goal:** a user can write multi-line JavaScript for any wire in a large Monaco editor, reopen it later, and see its `console.log` output in the Console panel during a run.

## 2. Decisions

| Question | Decision |
|---|---|
| Where the editor lives | A centred shadcn **Dialog** with a **Monaco** JavaScript editor. The docked side panel was offered and declined. |
| Test / preview button | **No.** Out of scope. |
| Script form | One expression (today's form), or several lines that end with `return value`. "Last line wins" and a `wire.setValue()` call were offered and declined. |
| Where logs appear | The existing Console panel, during a run. |
| Editing existing wires | Double-click a wire. |

## 3. Script evaluation (backend)

### 3.1 Result rule: one expression, or a script that ends with `return`
A wire script is either **one expression** (today's form) or **several lines that send their value with `return value`**. Detection happens inside the sandbox with the real JS parser, never with Rust heuristics, and only parses; the chosen form runs once:
1. **One expression.** Try `new Function('response', 'return (' + SRC + '\n)')`. Every existing single-expression wire parses here and behaves exactly as today. This also keeps `{ a: 1 }` an object, not a block.
2. **One expression with a trailing `;`.** Retry step 1 with a trailing `;` removed, so `response.body;` still works.
3. **A script with `return`.** Use `new Function('response', SRC)`.
4. If every form fails, report the script-form `SyntaxError`.

A script with no `return` yields `undefined`, which is the existing "resolved to null/undefined" error for a wire. The dialog's help text says to end with `return`.

`SRC` is embedded as a JSON-encoded string literal (`serde_json::to_string`), so quotes, backticks and newlines in user code cannot break the wrapper. `new Function` is allowed in the sandbox (bootstrap.js already uses it).

### 3.2 The `response` object
Unchanged from `46cd63f8`: `{ status, statusText, headers, body, duration_ms }` built from `res`.

### 3.3 If / Switch coercion
Today the backend builds `!!(cond)` and `String(value)` as text (`flow_execution_service.rs` ~735, ~759), which cannot wrap a function body. The coercion moves into the wrapper and is applied to the call result: `!!f(response)` for If, `String(f(response))` for Switch, the raw result for wires. Existing If/Switch conditions keep their meaning. A body with no `return` yields `undefined`: an error for a wire (existing "resolved to null/undefined"), `false` for If, `"undefined"` for Switch.

### 3.4 Logs
- A Flow-only evaluator returns the value **and** the script's `console_entries`, including entries logged before a thrown error. `evaluate_var_expression` (used by the request Vars tab) keeps its signature and behaviour.
- The node that consumes the wire (or the If/Switch node) owns the logs. A Request node also carries its own pre/post script logs (`ExecuteRequestOutput.console_entries`), which Flow runs drop today.
- `rocket-shared` gains `FlowLogEntry { level: FlowLogLevel, message }` (`FlowLogLevel` = `log | warn | error`, lowercase on the wire). `FlowStepResult` and `DomainEvent::FlowStepCompleted` gain `logs: Vec<FlowLogEntry>` (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`). Failed and succeeded steps both carry them.
- Secrets stay redacted, as the engine already does for console output.

## 4. Frontend

### 4.1 Wire script dialog
`WireExpressionPopover` is replaced by `WireScriptDialog`:
- shadcn `Dialog`, centred, `max-w-3xl`, controlled `open`, no trigger element.
- A Monaco editor, `language='javascript'`, fixed height (about 320px), lazy-loaded with `Suspense` like `ScriptsTab`.
- IntelliSense for `response` via a Monaco extra lib (`declare const response: {...}`), registered on mount and disposed on unmount so it never leaks into the Scripts tab.
- The header-name field stays for `headers` wires, seeded from an existing `headers[Name].value` target.
- Save and Cancel buttons. Cancel keeps today's rule: an uncommitted new `headers` wire is removed.
- `DialogContent` carries `nokey`, so Backspace/Delete inside the dialog never deletes a node or wire. On close, focus returns to the canvas.
- A short help line above the editor: write one expression, or several lines that end with `return value`; `console.log` output appears in the Console.

### 4.2 Opening
- Drawing a data wire opens the dialog, as the popover does today.
- Double-clicking an existing data wire opens the dialog for it. Trigger ("Run when") wires have no expression and do not open it.

### 4.3 Logs
When the final run summary arrives, the Flow toolbar hands each step's `logs` to `FlowPane`, which pushes them to `useConsoleStore.addScriptEntries` with `requestName` = `<flow name> › <node label>`. Logs are pushed from the summary only, never from the live events, so they are never duplicated and a late event cannot drop them. A tab that reattaches to a run already in progress gets no logs; that is accepted. No new UI.

## 5. Out of scope
- A Test / preview button, and keeping node responses after a run.
- Editing If/Switch conditions in the dialog. They stay on the card; they gain multi-line support only through the backend rule.
- Undo/redo.

## 6. Testing
- **Backend, real Deno engine** (the fake engine hid the last two bugs): a single expression; a multi-line script with `return`; a runtime `SyntaxError` from user code reported once; an object literal `{a: 1}` expression; a trailing `// comment`; a body with no return (wire error, If false); source containing quotes, backticks and newlines; If/Switch coercion on a body; `console.log` entries returned, including before a throw.
- **Backend event:** `logs` reaches `FlowStepCompleted` and the summary step; empty logs are omitted from JSON.
- **Frontend:** the dialog opens centred on connect and on edge double-click; Save updates the edge expression; the header name is seeded for an existing header wire; Backspace in the editor deletes nothing; step logs reach the console store.
- **Manual:** in `yarn tauri dev`, Monaco inside the Dialog renders at the right size and its suggest widget shows (no Dialog+Monaco precedent exists on this WebKitGTK machine).
