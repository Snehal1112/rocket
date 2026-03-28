# Design: Fix HttpResponse.body Serialization at API Boundary

**Date:** 2026-03-09
**Status:** Approved

---

## Problem

`HttpResponse.body` is typed as `string`, but axios auto-parses JSON response
bodies into JavaScript objects. The Go backend returns a JSON envelope:

```json
{ "data": { "status": 200, "body": {"id": 1, "title": "foo"}, ... } }
```

When axios parses this, `response.data.data.body` is already an **object**, not
a string. This causes a React render error when the console tries to display it:

> Objects are not valid as a React child (found: object with keys {body, id,
> title, userId})

### Current State (wrong)

Two broken layers exist:

| Layer | Problem |
|---|---|
| `api.ts` line 74 | Assigns object to `body: string` without normalizing |
| `console.ts` lines 43–45 | `typeof` guard as a band-aid — wrong layer |

The band-aid in `console.ts` only protects the console. Any other consumer of
`HttpResponse` (`RequestBuilder` display, `HistoryEntry`, future features) is
still exposed.

---

## Design

### Fix point: `api.ts`, `sendRequest()` success path only

Add a private helper `normalizeBody` that enforces the `string` contract at the
one place where `HttpResponse` is constructed from raw axios data:

```ts
private normalizeBody(body: unknown): string {
  if (typeof body === 'string') return body
  if (body == null) return ''
  return JSON.stringify(body, null, 2)
}
```

Apply it at the return site:

```diff
- body: response.data.data.body,
+ body: this.normalizeBody(response.data.data.body),
```

The error path already uses `JSON.stringify(axiosError.response.data, null, 2)`
and does not need changing.

### Clean up console.ts

Remove the `typeof` guard — `res.body` is guaranteed a string at the API
boundary:

```diff
- responseBody: typeof res.body === 'string'
-   ? res.body
-   : JSON.stringify(res.body, null, 2),
+ responseBody: res.body,
```

### Add a test

In `console.test.ts`, add a case that passes an object body through `addEntry`
**after** it has been normalized by the API layer — confirming the store no
longer needs to guard.

---

## Affected Files

| File | Change |
|---|---|
| `frontend/src/lib/api.ts` | Add `normalizeBody` private method, apply at success return |
| `frontend/src/store/console.ts` | Remove `typeof` guard, restore `responseBody: res.body` |
| `frontend/src/store/__tests__/console.test.ts` | Update comment to clarify body is pre-normalized |

---

## What Is Not In Scope

- Changing `HttpResponse.body` type to `unknown` (would break all consumers)
- Fixing the error path in `sendRequest` (already uses `JSON.stringify`)
- Any UX changes to how the body is displayed in the console
