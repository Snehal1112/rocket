# Console Panel — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Add a Postman-style Console panel that slides up above the StatusBar, logging every request sent during the session with expandable request/response detail.

## Architecture

A Zustand in-memory store (`useConsoleStore`) holds up to 200 console entries. The `sendRequest` function in `execute-request.ts` adds an entry after each request completes. A resizable `ConsolePanel` component renders between the main content area and `StatusBar` in `App.tsx`. `StatusBar` gains a Console toggle button.

## Section 1 — Console Store (`src/stores/console-store.ts`)

### ConsoleEntry type

```ts
interface ConsoleEntry {
  id: string;
  timestamp: string;
  method: string;
  url: string;
  status: number;
  statusText: string;
  durationMs: number;
  sizeBytes: number;
  requestHeaders: { key: string; value: string }[];
  requestBody: string;
  responseHeaders: { key: string; value: string }[];
  responseBody: string;
}
```

### Store interface

```ts
interface ConsoleState {
  entries: ConsoleEntry[];
  addEntry: (entry: Omit<ConsoleEntry, 'id' | 'timestamp'>) => void;
  clearEntries: () => void;
}
```

- `addEntry` prepends a new entry with auto-generated `id` and `timestamp`. Caps list at 200, dropping oldest.
- `clearEntries` empties the list.

## Section 2 — Data Flow (`src/lib/execute-request.ts`)

After `usePaneStore.getState().setResponse(tabId, responseState)` succeeds, call:

```ts
useConsoleStore.getState().addEntry({
  method: request.method,
  url: resolvedUrl,
  status: result.status,
  statusText: result.statusText,
  durationMs: result.durationMs,
  sizeBytes: result.sizeBytes,
  requestHeaders: resolvedHeaders.map((h) => ({ key: h.key, value: h.value })),
  requestBody: resolvedBody?.content ?? '',
  responseHeaders: result.headers.map((h) => ({ key: h.key, value: h.value })),
  responseBody: result.body,
});
```

Also log failed requests (catch block) with `status: 0`.

## Section 3 — Console Panel (`src/components/layout/ConsolePanel.tsx`)

### Props

```ts
interface ConsolePanelProps {
  isOpen: boolean;
  height: number;
  onHeightChange: (height: number) => void;
}
```

### Layout

- Returns `null` when `!isOpen`
- Fixed height container with drag handle at top edge (4px strip, `cursor-row-resize`)
- Toolbar row: "Console" label, entry count badge, spacer, URL filter input, Clear button
- Scrollable entry list below toolbar

### Entry row

Each row shows: `[HH:MM:SS] METHOD URL STATUS Xms`
- Status color: green (2xx), yellow (3xx), orange (4xx), red (5xx/0)
- Click expands to show request/response headers and body in a 2-column grid
- Chevron indicates expand state

### Height constraints

Default: 280px. Clamped: 120px min, 600px max. Drag handle resizes via pointer events.

## Section 4 — StatusBar Toggle (`src/components/layout/StatusBar.tsx`)

Add props:

```ts
interface StatusBarProps {
  isConsoleOpen?: boolean;
  onConsoleToggle?: () => void;
}
```

Add a "Console" button on the left side of the bar. Active state: `bg-accent` when open.

## Section 5 — App.tsx Wiring

Add state:
```ts
const [isConsoleOpen, setIsConsoleOpen] = useState(false);
const [consoleHeight, setConsoleHeight] = useState(280);
```

Insert `<ConsolePanel>` between the closing `</div>` of the main flex row and `<StatusBar>`:
```tsx
<ConsolePanel isOpen={isConsoleOpen} height={consoleHeight} onHeightChange={setConsoleHeight} />
<StatusBar isConsoleOpen={isConsoleOpen} onConsoleToggle={() => setIsConsoleOpen((o) => !o)} />
```

## Files Changed

| File | Changes |
|---|---|
| `src/stores/console-store.ts` | Create — console entry store with 200-entry cap |
| `src/lib/execute-request.ts` | Modify — add console entry after request completion |
| `src/components/layout/ConsolePanel.tsx` | Create — resizable console panel with entry list |
| `src/components/layout/StatusBar.tsx` | Modify — add Console toggle button |
| `src/App.tsx` | Modify — wire console state and render panel |

## Out of Scope

- Persistent storage across app restarts
- Script/console.log output
- Network timing waterfall
- Export console entries
- Console keyboard shortcut (can add later)
