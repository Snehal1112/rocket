# Runtime Assertions Design

**Date:** 2026-05-20
**Status:** Approved

## Overview

Implement declarative runtime assertions conforming to the Bruno OpenCollection spec (`runtime.assertions`). Assertions evaluate response data using a fixed operator set and produce `TestResult` entries that surface in the existing `TestsPanel` — no new display infrastructure needed. The evaluator is pure Rust in `rocket-app`, keeping all domain logic out of the frontend and infrastructure layers.

---

## Current State

| Layer | Status |
|---|---|
| `Assertion` domain type (`rocket-shared`) | Exists — `expression`, `operator`, `value`, `disabled`, `description` |
| YAML persistence (`runtime.assertions`) | Exists — roundtrips correctly via `OcHttpRequestRuntime` |
| `Request.assertions: Vec<Assertion>` | Exists — carried through save/load |
| Assertion evaluator | **Missing** |
| `ExecuteRequestInput.assertions` | **Missing** |
| Frontend state (`RequestState.assertions`) | **Missing** |
| Assertions UI tab | **Missing** |

---

## Architecture

```
AssertionsTab (new React component)
    ↓ updateRequest({ assertions })  →  RequestState.assertions
SaveRequestButton / auto-save
    ↓ assertions: tab.request.assertions
tauri-api.ts Request interface
    ↓ IPC
save_request Tauri command → FsCollectionRepo → YAML (already works)

execute-request.ts
    ↓ assertions: request.assertions  added to ExecuteRequestInput payload
execute_request Tauri command
    ↓
ExecuteRequestInput.assertions (new field, #[serde(default)])
    ↓
execution_service::execute()
    ↓ after tests-script phase
evaluate_assertions(&assertions, &response) → Vec<TestResult>
    ↓ appended to all_test_results
ExecuteRequestOutput.test_results → TestsPanel (no changes needed)
```

---

## Section 1 — Assertion Evaluator (`rocket-app`)

### Location

New file: `crates/rocket-app/src/assertion_evaluator.rs`

### Signature

```rust
pub fn evaluate_assertions(
    assertions: &[Assertion],
    response: &HttpResponse,
) -> Vec<TestResult>
```

### Expression Resolution

Expressions follow the `res.*` path convention. Resolution order:

| Expression | Value |
|---|---|
| `res.status` | HTTP status code (integer) |
| `res.responseTime` | Duration ms (integer) |
| `res.body` | Full response body string |
| `res.body.<path>` | JSON pointer into parsed body (`res.body.token`, `res.body.user.id`) |
| `res.headers.<name>` | Response header value, case-insensitive lookup |

Unresolvable expressions (e.g. `res.body.x` on non-JSON body, unknown prefix) produce a `TestResult` with status `Failed` and an error message describing why resolution failed.

### Operator Set (full Bruno compatibility)

| Category | Operator strings |
|---|---|
| Equality | `eq`, `neq` |
| Numeric | `gt`, `gte`, `lt`, `lte` |
| String | `contains`, `notContains`, `startsWith`, `endsWith`, `matches`, `notMatches` |
| Type checks | `isString`, `isNumber`, `isBoolean`, `isArray`, `isNull`, `isDefined`, `isUndefined` |
| Value checks | `isEmpty`, `isNotEmpty`, `isTruthy`, `isFalsy`, `isJson` |
| Set | `in`, `notIn` |
| Range | `between` |
| Length | `length` |

**Unary operators** (`isString`, `isNumber`, `isBoolean`, `isArray`, `isNull`, `isDefined`, `isUndefined`, `isEmpty`, `isNotEmpty`, `isTruthy`, `isFalsy`, `isJson`) — `value` field is ignored.

**`between`** — `value` contains two comma-separated numbers, e.g. `"100,999"`.

**`in` / `notIn`** — `value` contains a JSON array string, e.g. `"[200, 201, 204]"`.

**`length`** — compares the `.len()` of the resolved string/array against `value` as a number.

**`matches` / `notMatches`** — `value` is a regex string.

### Result Name

Auto-generated as `"{expression} {operator} {value}"` for binary operators, `"{expression} {operator}"` for unary. Example: `"res.status eq 200"`, `"res.body.token isDefined"`.

### Disabled Assertions

Assertions with `disabled: Some(true)` are skipped — no `TestResult` emitted.

### Numeric Coercion

When an operator is numeric (`gt`, `gte`, `lt`, `lte`, `eq`, `neq`, `between`, `length`) and the resolved value is a string that parses as a number, it is coerced. This handles `res.status` (stored as u16, compared against the string `"200"`).

---

## Section 2 — IPC and State Changes

### `ExecuteRequestInput` (`rocket-app/src/execution_service.rs`)

```rust
#[serde(default)]
pub assertions: Vec<Assertion>,
```

No breaking change — old payloads without the field default to empty vec.

### `RequestState` (`src/types/pane-types.ts`)

```typescript
assertions: AssertionEntry[];   // imported from tauri-api.ts
```

### `tauri-api.ts` — `AssertionEntry` type and `Request` interface

Define `AssertionEntry` in `tauri-api.ts` and export it so `pane-types.ts` can import it:

```typescript
export interface AssertionEntry {
  expression: string;
  operator: string;
  value?: string;
  disabled?: boolean;
}
```

Add to `Request` interface:

```typescript
assertions?: AssertionEntry[];
```

### `pane-utils.ts` — `mapApiRequestToState`

```typescript
assertions: req.assertions ?? [],
```

### `pane-utils.ts` — `createDefaultRequest`

```typescript
assertions: [],
```

### `SaveRequestButton.tsx` — `buildPayloadFromTab`

```typescript
assertions: tab.request.assertions ?? [],
```

### `auto-save.ts` — `toApiRequest`

```typescript
assertions: request.assertions ?? [],
```

### `execute-request.ts`

Add to the `ExecuteRequestInput` payload:

```typescript
assertions: effectiveRequest.assertions ?? [],
```

---

## Section 3 — Assertions UI

### New Component: `AssertionsTab.tsx`

Location: `src/components/request/AssertionsTab.tsx`

A table-style row editor. Each row represents one assertion:

| Column | Component | Notes |
|---|---|---|
| Enabled | shadcn `Switch` | Maps to `!disabled` |
| Expression | `SingleLineEditor` | CodeMirror 6, variable-aware |
| Operator | shadcn `Select` | Grouped by category (Equality, Numeric, String, Type, Value, Set) |
| Value | `SingleLineEditor` | Hidden/disabled for unary operators |
| Delete | lucide `Trash2` `Button` | Removes the row |

Footer: `+ Add Assertion` button (shadcn `Button`, variant `ghost`) appends a blank row with `expression: ''`, `operator: 'eq'`, `value: ''`.

Props:

```typescript
interface AssertionsTabProps {
  assertions: AssertionEntry[];
  onChange: (assertions: AssertionEntry[]) => void;
}
```

### `RequestPanel.tsx` changes

1. Add `'assertions'` to `SectionTab` union type.
2. Add tab definition in `tabDefs` after `scripts`, before `docs`:
   - Label: `Assertions`
   - Dot indicator when `request.assertions.some(a => !a.disabled)` is true
3. Render `<AssertionsTab>` when `activeSection === 'assertions'`.
4. The assertions section uses the full-panel layout (same as Scripts, Docs — no request/response split).

### Dirty Tracking

Any change via `AssertionsTab.onChange` calls:

```typescript
updateRequest(tab.id, { assertions: newAssertions })
```

This marks the tab dirty and triggers auto-save, identical to all other request fields.

---

## Section 4 — Execution Wiring

In `execution_service::execute()`, after the tests-script phase and before event emission:

```rust
// ── Declarative assertions ────────────────────────────────────────────
let assertion_results = evaluate_assertions(&input.assertions, &response);
all_test_results.extend(assertion_results);
```

Assertions always run after the tests script so JS test results appear first in the panel (preserving the existing ordering users expect from the tests script).

---

## Section 5 — Testing

### Rust unit tests (`crates/rocket-app/src/assertion_evaluator.rs`)

- Each operator: at least one passing case and one failing case
- Expression resolution: `res.status`, `res.body` (string), `res.body.field` (JSON), `res.headers.x-foo`, `res.responseTime`
- Disabled assertions produce no results
- Unresolvable expression → `TestResult::Failed` with descriptive error
- Numeric coercion: status code `200` compared with `gt 100`
- `between`: range pass and fail
- `in` / `notIn`: array membership
- `matches`: valid and invalid regex

### Integration test (`crates/rocket-app/src/execution_service.rs`)

One test using `sample_input()` extended with `assertions: vec![Assertion::new("res.status", "eq", Some("200".into()))]`, asserting the result appears in `ExecuteRequestOutput.test_results`.

### Verification commands

```bash
cargo test -p rocket-app assertion
yarn tsc --noEmit
yarn check
```

---

## Out of Scope

- Assertions in the collection runner (future work)
- Assertions in load tests (future work)
- Custom expression functions beyond `res.*` paths
- UI for `description` field on individual assertions (persists from YAML but not editable in this iteration)
