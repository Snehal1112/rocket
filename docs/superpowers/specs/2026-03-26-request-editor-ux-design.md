# Request Editor UX Improvements — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Improve request tab usability with shared KeyValueEditor, pre-send validation, loading states, save feedback, and unsaved changes warning.

## Section 1 — Shared KeyValueEditor component

### Problem

Headers, QueryParams, PathParams, and FormData editors all duplicate the same pattern: checkbox + key input + value input + delete button + "Add" button. Each is ~80 lines of nearly identical code.

### Solution

Extract `src/components/request/KeyValueEditor.tsx` — a single reusable component.

```tsx
interface KeyValueEditorProps {
  entries: KeyValueEntry[];
  onChange: (entries: KeyValueEntry[]) => void;
  keyPlaceholder?: string;     // e.g., "Header name", "Param name"
  valuePlaceholder?: string;   // e.g., "Value"
  addLabel?: string;           // e.g., "Add Header", "Add Param"
}
```

**Behavior:**
- Renders a list of rows, each with: enable/disable checkbox, key input, value input, delete (Trash2) button
- "Add" button at the bottom appends a new empty entry with `id: crypto.randomUUID()`, `enabled: true`
- Deleting an entry removes it by `id`
- Toggling checkbox updates `enabled` field
- Key/value changes update in place
- Calls `onChange` with the full updated array on every change

**After extraction:**
- `HeadersEditor.tsx` becomes a thin wrapper: `<KeyValueEditor entries={headers} onChange={...} keyPlaceholder="Header name" addLabel="Add Header" />`
- `QueryParamsEditor.tsx` same pattern with "Param name" / "Add Query Param"
- `PathParamsPanel.tsx` same pattern with "e.g. customerId" / "Add Path Param"
- `BodyEditor.tsx` FormData section uses `<KeyValueEditor>` internally

---

## Section 2 — Request validation before sending

### Problem

Users can click Send with an empty URL, invalid URL, or missing auth fields. No feedback — the request either fails silently or produces a confusing error.

### Solution

Add inline validation in `RequestPanel.tsx` before calling `sendRequest`.

**Validation rules:**
1. **URL required** — if empty, show `"URL is required"` below the URL input
2. **URL format** — if not parseable as a URL (no protocol), show `"Invalid URL — include http:// or https://"`
3. **Basic auth** — if authType is `basic` and username is empty, show badge on Auth tab

**Implementation:**
- Add `urlError` state to RequestPanel: `const [urlError, setUrlError] = useState('')`
- On Send click: validate URL first. If invalid, set error and return early (don't send)
- On URL change: clear the error (`setUrlError('')`)
- Error displayed as `<p className="text-2xs text-destructive px-3">` below the URL bar
- Auth tab badge turns red when auth fields are incomplete (text-destructive instead of text-muted-foreground)

**No validation for:**
- Header/param values (too restrictive for an API tool)
- Body content (JSON validity is a nice-to-have, not a blocker)

---

## Section 3 — Loading state on response area

### Problem

When a request is in-flight, only the Send button shows "Sending...". The response area shows stale data or the empty placeholder. No visual indicator that work is happening.

### Solution

- When `sending` is true, the response area shows a centered spinner: `<Loader2 className="h-5 w-5 animate-spin" />` with "Sending request..." text below
- The URL input and method selector get `opacity-60 pointer-events-none` during send (prevents editing mid-flight)
- When response arrives, spinner is replaced by the response viewer

**Implementation:**
- The `sending` state already exists in `useExecuteRequest` hook
- Add a conditional render in RequestPanel: `sending ? <SendingState /> : <ResponseViewer />`
- The `<SendingState>` is a simple centered div with spinner + text (inline, not a separate file)

---

## Section 4 — Save success/failure feedback

### Problem

After saving, no feedback. Users don't know if save succeeded or failed. Errors only logged to console.

### Solution

Add inline feedback next to the Save button in `SaveRequestButton.tsx`.

- **Success**: Show a green `Check` icon for 2 seconds next to the button, then fade out
- **Failure**: Show `"Save failed"` in `text-destructive text-2xs` next to the button for 3 seconds

**Implementation:**
- Add `saveStatus` state: `'idle' | 'success' | 'error'`
- On successful save: set `'success'`, clear after 2s via `setTimeout`
- On failed save: set `'error'`, clear after 3s
- Render feedback inline next to the button (no toast library)

---

## Section 5 — Unsaved changes warning on tab close

### Problem

Closing a tab with unsaved changes silently discards the work. The unsaved changes AlertDialog exists in RequestPanel but is never triggered.

### Solution

When `closeTab` is called for a tab where `isDirty: true` AND `source` is undefined (request was never saved to a collection), show a confirmation dialog.

**Implementation approach:**
- Add a `pendingCloseTabId` state to the component that renders the AlertDialog (EditorGroup or a new wrapper)
- Intercept `closeTab` — if tab is dirty and has no source, set `pendingCloseTabId` instead of closing immediately
- AlertDialog shows: "Unsaved changes will be lost. Close anyway?"
- "Close" button: calls `closeTab` for real
- "Cancel" button: clears `pendingCloseTabId`
- Tabs WITH a source auto-save on close (existing behavior) — no warning needed

---

## Files Changed

| File | Changes |
|---|---|
| `src/components/request/KeyValueEditor.tsx` | Create — shared key-value editor component |
| `src/components/request/HeadersEditor.tsx` | Simplify — use KeyValueEditor |
| `src/components/request/QueryParamsEditor.tsx` | Simplify — use KeyValueEditor |
| `src/components/request/PathParamsPanel.tsx` | Simplify — use KeyValueEditor |
| `src/components/request/BodyEditor.tsx` | Simplify FormData section — use KeyValueEditor |
| `src/components/request/RequestPanel.tsx` | Add URL validation, loading state, send spinner |
| `src/components/request/SaveRequestButton.tsx` | Add success/error feedback |
| `src/components/panes/EditorGroup.tsx` | Add unsaved changes warning dialog |
| `src/stores/pane-store.ts` | Expose close-with-warning flow |

## Out of Scope

- JSON body validation (nice-to-have, separate effort)
- Header autocomplete/suggestions
- Pre-request scripts
- Response search/filter
- Tab management improvements (Area B)
- Missing HTTP features like file upload (Area C)
- Response viewer enhancements (Area D)
