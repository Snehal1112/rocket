# Request Layout Toggle Design

**Date:** 2026-04-25  
**Status:** Approved

## Summary

Add a global toggle to switch the request panel between stacked (request top, response bottom) and side-by-side (request left, response right) layouts. The toggle lives in the bottom status bar. The preference is persisted to `ui-state.yml` via the existing Tauri persistence path.

---

## Architecture

State flows through a new lightweight Zustand store (`useLayoutStore`) that is initialized from the restored `UiState` on app startup and written back via the existing debounced `scheduleSaveUiState`.

```
StatusBar button click
  → useLayoutStore.setRequestLayout('side-by-side')
    → scheduleSaveUiState() (debounced 500ms)
      → save_ui_state Tauri command → ui-state.yml

App startup
  → restoreUiState()
    → useLayoutStore.setRequestLayout(saved value)

RequestPanel
  → reads useLayoutStore.requestLayout
    → renders flex-row (side-by-side) or flex-col (stacked)
```

---

## Persistence

Stored in the existing `ui-state.yml` in the Tauri app config dir (`~/.config/rocket-api/ui-state.yml`). A new optional field is added:

```yaml
activeMode: collection
layoutDirection: side-by-side  # new field; absent = stacked (default)
```

No migration needed — absent field defaults to `'stacked'`, preserving existing behavior for all users.

---

## Data Changes

### Rust — `src-tauri/src/commands/ui_state.rs`

Add one optional field to `UiState`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub layout_direction: Option<String>, // "stacked" | "side-by-side"
```

### TypeScript — `src/lib/tauri-api.ts`

Add to the `UiState` interface:

```ts
layoutDirection?: 'stacked' | 'side-by-side';
```

### New Zustand store — `src/stores/layout-store.ts`

```ts
interface LayoutStore {
  requestLayout: 'stacked' | 'side-by-side';
  setRequestLayout: (dir: 'stacked' | 'side-by-side') => void;
}
```

Default value: `'stacked'`.

### `src/lib/ui-state.ts`

- `scheduleSaveUiState` reads `requestLayout` from `useLayoutStore` and includes it as `layoutDirection` in the `UiState` object passed to `saveUiState`.
- No changes to `restoreUiState` itself — the caller (`App.tsx`) reads the returned `layoutDirection` and calls `setRequestLayout`.

### `src/App.tsx`

In the `init` function, after `restoreUiState()`:

```ts
if (uiState?.layoutDirection) {
  useLayoutStore.getState().setRequestLayout(uiState.layoutDirection);
}
```

---

## Component Changes

### `RequestPanel.tsx`

- Reads `requestLayout` from `useLayoutStore`.
- When `'stacked'`: existing behavior unchanged (flex-col, horizontal drag separator).
- When `'side-by-side'`:
  - URL bar stays full-width at the top.
  - Below the URL bar: `flex-row` — request tabs+body on the left, response on the right.
  - Separator becomes vertical; drag logic uses `clientX` instead of `clientY`.
  - Initial split: 50/50.

### `StatusBar.tsx`

- Reads and writes `requestLayout` from `useLayoutStore`.
- Renders a ghost `Button` in the bottom-right of the status bar.
- **Stacked mode:** `PanelBottom` icon (lucide-react), label "Side by side"
- **Side-by-side mode:** `PanelRight` icon (lucide-react), label "Stack", highlighted (active style)

---

## Behavior

- Default is `'stacked'` — no change for existing users.
- Toggle is immediate (no animation needed).
- Save is debounced 500ms via existing `scheduleSaveUiState` — no new save path.
- No flash on startup: `restoreUiState()` runs before UI renders in `App.tsx`.
- The layout preference is global (applies to all request tabs in all collections).
