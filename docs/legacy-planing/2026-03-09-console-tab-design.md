# Design: Console Tab in Bottom Status Bar

**Date**: 2026-03-09

## Goal

Add a Postman-style Console panel that slides up above the `GlobalStatusBar`. It shows
a log of every request sent during the session, with expandable rows for full
request/response detail.

## Approach

Option A: Zustand in-memory store + App.tsx wiring. The existing `onRequestSent`
callback in `App.tsx` feeds entries into the store. No backend changes. No changes to
`useRequestBuilderState`.

---

## Data Model

New type `ConsoleEntry` added to `frontend/src/types/index.ts`:

```ts
export interface ConsoleEntry {
  id: string                          // crypto.randomUUID()
  timestamp: string                   // ISO string
  method: HttpMethod
  url: string
  status: number
  statusText: string
  duration: number                    // ms (from HttpResponse.time)
  size: number                        // bytes (from HttpResponse.size)
  requestHeaders: Record<string, string>
  requestBody: string
  responseHeaders: Record<string, string>
  responseBody: string
}
```

---

## Store

New file `frontend/src/store/console.ts` — Zustand in-memory store:

- `entries: ConsoleEntry[]`
- `addEntry(req: HttpRequest, res: HttpResponse): void`
  - Maps HttpRequest + HttpResponse → ConsoleEntry
  - Only includes enabled request headers
  - Caps list at 200 entries (drops oldest)
- `clearEntries(): void`

---

## Layout

App.tsx flex column becomes:

```
header (h-14)
flex-1 row
  CollectionsSidebar
  main (RequestTabs + RequestBuilder / CollectionOverview)
ConsolePanel (resizable, default 280px, hidden when closed)
  drag handle (4px strip at top edge, mousedown to resize)
GlobalStatusBar (h-7, Console toggle button on right side)
```

Panel height is clamped between 120px and 600px via `useState` in `App.tsx`.
`isConsoleOpen` boolean state also lives in `App.tsx`.

---

## ConsolePanel Component

New file `frontend/src/components/layout/ConsolePanel.tsx`:

- Props: `isOpen: boolean`, `height: number`, `onHeightChange: (h: number) => void`
- Tab strip at top: single "Console" tab (extensible later)
- Toolbar: entry count, "Clear" button, text search input (filters by URL)
- Entry list: each row shows `[HH:MM:SS] METHOD URL → STATUS Xms`
  - Status color: green (2xx), yellow (3xx/4xx client), red (5xx/network error)
  - Clicking a row expands it to show request headers, request body, response
    headers, response body in a 2-column grid
- Empty state: "No requests sent yet" message

---

## Data Flow

In `App.tsx`, update `onRequestSent`:

```tsx
onRequestSent={(req, res) => {
  useConsoleStore.getState().addEntry(req, res)
}}
```

`ConsolePanel` reads entries via `useConsoleStore()`.

---

## GlobalStatusBar Change

Add a "Console" toggle button to the right side of the `GlobalStatusBar` strip. It
receives `isConsoleOpen` and `onToggle` as props from `App.tsx`.

---

## Testing

1. `frontend/src/store/__tests__/console.test.ts`
   - `addEntry` populates entries correctly
   - `clearEntries` empties the list
   - Max-200 cap drops the oldest entry

2. `frontend/src/components/layout/__tests__/ConsolePanel.test.tsx`
   - Renders entry list from store
   - Clicking a row expands headers/body
   - Clear button calls `clearEntries`
   - Hidden when `isOpen=false`

3. Existing `App.tsx` tests: after mocked `sendRequest`, verify store has one entry.

---

## Out of Scope

- `pm.console.log()` / script log output (future feature)
- Persistent storage across page reloads
- Network-level request inspection (timing waterfall, etc.)
