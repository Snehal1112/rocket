# History resend keeps the process env trust gate

Ticket: https://github.com/Snehal1112/rocket/issues/116

## Goal

Make a request resent from History follow the same host environment trust gate as the collection send it came from, so an untrusted collection cannot use History to read `{{process.env.NAME}}` values from the host.

## Overview

A collection send of an untrusted collection withholds `{{process.env.NAME}}`. A placeholder that does not resolve stays as written, and History stores the resolved URL with the placeholder still in it. Reopening that entry makes a History tab with no collection. A request with no collection is treated as scratch and gets the full host environment, so pressing Send resolves the placeholder and sends the value to the attacker's host.

Investigation of the chain (all links checked in source):

- The ticket's exact example, a placeholder in the query string, does not leak with the default `encodeUrl: true`. Only the base URL is resolved and recorded, and the query goes into `queryParams`, which History does not store (`src/lib/execute-request.ts:379-395`, `crates/rocket-app/src/execution_service.rs:2803-2815`).
- A placeholder in the URL path, or in the query with `encodeUrl: false`, does leak. The denied send records the literal placeholder, and the resend resolves it.
- The backend already stores the collection name on a History entry and already denies a named collection that is untrusted or unknown. No backend change is needed.

## Current state

| Area | Today | Where |
|---|---|---|
| History tab | Built from `entry.method` and `entry.url` only. No `source`, no collection | `src/components/history/HistoryPanel.tsx:149-163` |
| Tab type | `RequestTab` has no field for a collection except `source`, which also drives saving | `src/types/pane-types.ts:25-33` |
| Send | Reads the collection from `found?.tab.source?.collection` | `src/lib/execute-request.ts:696-699` |
| Field resolution | `resolveRequestFields` passes only `source` to `resolveRequestFieldsForPath` | `src/lib/execute-request.ts:477-490` |
| Process env | A null collection fetches the full map | `src/lib/execute-request.ts:103-119`, `src-tauri/src/commands/environments.rs:397-412` |
| Warning | Skipped when there is no collection | `src/lib/process-env-gate.ts:24` |
| Preview hook | `useProcessEnvVars(tab.source?.collection)` | `src/components/request/RequestPanel.tsx:572` |
| History entry | Has `collection?: string`, set when the send has a collection and a request name | `src/lib/tauri-api.ts:506-517`, `execution_service.rs:2811-2815` |

Setting `source` on a History tab is wrong. It would turn on autosave to a request file, change the save buttons and breadcrumb, switch the active collection, and load collection variables, headers and auth into the send.

## Non-goals

- Redacting host environment values that History stores in clear text when the gate allows access. This is a separate ticket (see Follow-ups).
- Any backend change, any `src-tauri` change, any edit to `docs/features/collection-trust.md`.
- Changing what a History tab can edit or save.

## Design

Flow: `openEntry(entry) -> tab.historyCollection -> process env scope -> getProcessEnvVars`.

### 1. A separate field on the History tab

`RequestTab` gains `historyCollection?: string | null`. It is set only for `tabType: 'history'`.

- `entry.collection` present: `historyCollection = entry.collection`.
- `entry.collection` absent (old entries and scratch sends): `historyCollection = null`.

`source`, `transient`, `title` and `isDirty` stay as they are today.

### 2. Process env scope

In `src/lib/execute-request.ts`:

```ts
export const CLOSED_PROCESS_ENV = Symbol('closed-process-env');
export type ProcessEnvScope = string | typeof CLOSED_PROCESS_ENV | undefined;
```

- `getProcessEnvVars(scope)` returns `NO_PROCESS_ENV` at once for `CLOSED_PROCESS_ENV`. It makes no backend call.
- One exported helper, `processEnvScopeOf(tab: Tab): ProcessEnvScope`, owns the rule. `resolveRequestFields`, `sendRequest` and `RequestPanel` all call it. The rule: `source?.collection` if the tab has a `source` (it wins, and `historyCollection` is left alone, so a History tab saved into a collection follows the collection), else for a `history` tab `historyCollection ?? CLOSED_PROCESS_ENV`, else `undefined` (scratch, full access as today).
- `getProcessEnvVars` is module-private, so tests reach it through `buildOAuth2VarContext(collection, path, scope)` and `resolveRequestFields` with a pane-store tab.
- `resolveRequestFieldsForPath(collection, path, request, forSend, processEnvScope?)` and `buildOAuth2VarContext(collection, path, processEnvScope?)` gain a last optional parameter. When it is omitted they behave as today, using `collection`.
- `resolveRequestFields` and `sendRequest` compute the scope from the tab and pass it. Copy as cURL, the load test and GraphQL introspection call `resolveRequestFields`, so they inherit it.
- The scope is used only for the process env. Collection variables, headers and auth still come from `collection`, which stays `undefined` for a History tab.

Example: a History tab with `historyCollection: 'clone'` asks `getProcessEnvVars('clone', null, null)`. The backend returns an empty map if `clone` is not trusted. A tab with `historyCollection: null` gets no host environment and the placeholder stays unresolved.

### 3. Warning

`warnIfProcessEnvWithheld(scope, parts, requestName)` widens its first parameter to `ProcessEnvScope` and handles `CLOSED_PROCESS_ENV`: when the request text contains `{{process.env.`, it shows the same console entry and toast with this message: `Host environment variables are not available for a request reopened from History without a collection, so {{process.env.NAME}} was sent unresolved.` It has no Review action. For a collection scope it behaves as today.

### 4. Preview

A symbol cannot be a query key and hooks cannot be called conditionally, so `useProcessEnvVars(collection, enabled = true)` in `src/lib/queries/environment-queries.ts` gains an `enabled` flag that is passed to `useQuery`. `RequestPanel.tsx` calls it with the scope's collection string and `enabled: scope !== CLOSED_PROCESS_ENV`, and uses an empty map for a closed tab, so variable tooltips never show a host value.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| History entry from a trusted collection | Placeholder resolves as in the original send |
| History entry from an untrusted collection | Placeholder stays unresolved, console entry and toast |
| Old entry with no collection | Closed scope, placeholder unresolved, console entry and toast |
| Entry whose collection was renamed or deleted | The backend treats an unknown name as untrusted, so withheld |
| Scratch tab (not History) | Full access as today |
| Request with no `{{process.env.` text | No warning, no change |
| Tab opened twice for the same entry | Same id `history-<id>`, existing dedup, unchanged |
| Entry recorded with a collection but no request name | The backend then stores no collection, so the tab is closed |
| The resend itself is recorded | It passes no collection to the backend, so the new entry has none and reopens closed. This is the safe direction and is accepted |
| History tab saved into a collection | `source` is set, so `source.collection` wins over `historyCollection` |
| User types a pre-request script in a History tab | Unchanged. A scratch send gets only what the backend gives scratch scripts, and the spec does not change what a History tab can edit |
| `getProcessEnvVars` call fails | `NO_PROCESS_ENV`, as today |

## Testing

All three test files already exist, so the builder appends to them and names each test as a plain sentence, like the existing `it('...')` cases.

- `src/lib/__tests__/process-env-scope.test.ts` (mocks `getProcessEnvVars`, asserts `toHaveBeenCalledWith('clone', null, null)`): `asks for the process env of the history collection`, `gets no host environment and makes no call for a history tab without a collection`, `keeps full access for a scratch tab`, `does not load collection variables or headers for a history tab`, `lets source win over historyCollection`.
- `src/lib/__tests__/process-env-gate.test.ts`: `warns for a closed scope when the request uses process.env`, `stays silent for a closed scope without the placeholder`.
- `src/components/history/__tests__/HistoryPanel.test.tsx`: `sets historyCollection and no source when the entry has a collection`, `sets historyCollection to null when the entry has none`.
- `src/lib/queries/environment-queries.ts`, if it has a test file: `does not call the backend when disabled`.
- Commands: `yarn test process-env-scope process-env-gate HistoryPanel`, `yarn tsc --noEmit`, `yarn check`.

## Files changed

- `src/types/pane-types.ts`
- `src/components/history/HistoryPanel.tsx`
- `src/lib/execute-request.ts`
- `src/lib/process-env-gate.ts`
- `src/components/request/RequestPanel.tsx`
- `src/lib/queries/environment-queries.ts`
- The three test files above.

## Constraints

- No `src-tauri`, `crates` or `docs/features/collection-trust.md` change. Those paths match the factory risk rules and would stop the job for a human.
- Do not set `source` on a History tab.
- Only the files listed. No formatting, clippy or import-order edits in other files.
- Frontend rules: shadcn/ui primitives, lucide-react icons, narrow Zustand selectors.
- Checks: `yarn tsc --noEmit`, `yarn check`, `yarn test`.

## Acceptance criteria

- A History tab built from an entry with a collection sends with that collection's process env scope, and the backend's trust decision applies.
- A History tab built from an entry without a collection gets no host environment and makes no process env call.
- The History tab has no `source` and does not autosave, switch collections or load collection variables.
- The withheld warning appears for both cases when the request text uses `{{process.env.`.
- Copy as cURL, the load test and GraphQL introspection from a History tab use the same scope.
- The variable preview of a closed History tab shows no host value.
- A scratch tab that is not a History tab keeps full access.
- `yarn tsc --noEmit`, `yarn check` and the targeted tests pass.

## Out of scope

- Editing `docs/features/collection-trust.md` (done by hand after the merge).
- Redacting stored host values.
- Keeping the collection on the entry that a resend itself records.
- Changing callers that need no edit because the new parameter is optional: `flow-auth-preflight.ts`, `flow-auth.ts`, `runner-execute.ts`.

## Follow-ups

- Redact host environment values before they are written to History, so a path placeholder resolved with an allowed or scratch send does not store the real value in clear text.

## Decisions

- What do old History entries with no collection get? Options: close the gate; leave them as scratch; ask before resolving. Chosen: close the gate. Decided by: user
- Does this job also stop History from storing host values in clear text? Options: separate ticket; fold it in. Chosen: separate ticket. Decided by: user
- What does the user see when the gate withholds on a resend? Options: same warning as other sends; silent. Chosen: same warning channels (console entry and toast). Decided by: user
- How does the docs line change? Options: edit by hand after the merge; put it in the job. Chosen: by hand after the merge, because `collection-trust.md` matches the `*trust*` risk rule. Decided by: user
- How does the History tab carry the collection? Chosen: a separate `historyCollection` field, never `source`, because `source` turns on autosave, the save UI, collection switching and collection variables. Decided by: call
- How is the closed gate implemented? Chosen: a `CLOSED_PROCESS_ENV` scope that returns an empty map in the frontend with no backend call, instead of a made-up collection name. Decided by: call
- Is the closed-scope warning text the same as the collection text? Chosen: a different message without a Review action, because there is no collection to review. Decided by: call
- Does the preview hook change? Chosen: yes, a closed History tab uses an empty map. Decided by: call
- What wins when a History tab also has a `source`? Chosen: `source` wins and `historyCollection` is left alone, so a History tab saved into a collection follows that collection. Decided by: call
- Where does the scope rule live? Chosen: one exported `processEnvScopeOf(tab)` helper in `execute-request.ts`, used by the send, field resolution and `RequestPanel`. Decided by: call
- How does the closed scope reach `useProcessEnvVars`? Chosen: an `enabled` flag on the hook, because a symbol cannot be a query key and hooks cannot be called conditionally. Decided by: call
- Does the entry that a resend records keep the collection? Chosen: no. It reopens closed, which is the safe direction. Decided by: call
