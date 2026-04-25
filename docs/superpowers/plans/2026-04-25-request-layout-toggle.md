# Request Layout Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a global toggle in the status bar to switch the request panel between stacked (request top, response bottom) and side-by-side (request left, response right) layouts, persisted to `ui-state.yml`.

**Architecture:** A new Zustand store (`useLayoutStore`) holds `requestLayout`. It is initialized from `ui-state.yml` on startup via `restoreUiState()` in `App.tsx`, and written back via the existing debounced `scheduleSaveUiState`. `RequestPanel` reads the store to decide its flex direction; `StatusBar` reads and writes it for the toggle button.

**Tech Stack:** React, TypeScript, Zustand, Tauri (Rust + serde_yaml), lucide-react, shadcn/ui Button, Vitest

---

## File Map

| Action | File |
|--------|------|
| **Modify** | `src-tauri/src/commands/ui_state.rs` — add `layout_direction` field |
| **Modify** | `src/lib/tauri-api.ts` — add `layoutDirection` to `UiState` interface |
| **Create** | `src/stores/layout-store.ts` — new Zustand store |
| **Create** | `src/stores/__tests__/layout-store.test.ts` — store tests |
| **Modify** | `src/lib/ui-state.ts` — include `layoutDirection` in save |
| **Modify** | `src/App.tsx` — initialize layout store from restored state |
| **Modify** | `src/components/layout/StatusBar.tsx` — add toggle button |
| **Modify** | `src/components/request/RequestPanel.tsx` — side-by-side layout branch |

---

## Task 1: Add `layout_direction` to Rust `UiState`

**Files:**
- Modify: `src-tauri/src/commands/ui_state.rs`

- [ ] **Step 1: Add the field**

Open `src-tauri/src/commands/ui_state.rs`. The current `UiState` struct is:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub active_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_tabs: Option<UiStateWorkspaceTabs>,
}
```

Replace it with:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub active_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_tabs: Option<UiStateWorkspaceTabs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_direction: Option<String>,
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/ui_state.rs
git commit -m "feat(ui-state): add layout_direction field to UiState"
```

---

## Task 2: Add `layoutDirection` to TypeScript `UiState`

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add the field to the interface**

Find the `UiState` interface (around line 885):

```ts
export interface UiState {
  activeMode: 'workspace' | 'collection';
  workspaceTabs?: UiStateWorkspaceTabs;
}
```

Replace with:

```ts
export interface UiState {
  activeMode: 'workspace' | 'collection';
  workspaceTabs?: UiStateWorkspaceTabs;
  layoutDirection?: 'stacked' | 'side-by-side';
}
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(ui-state): add layoutDirection to UiState TS interface"
```

---

## Task 3: Create the `useLayoutStore` Zustand store

**Files:**
- Create: `src/stores/layout-store.ts`
- Create: `src/stores/__tests__/layout-store.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/stores/__tests__/layout-store.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { useLayoutStore } from '../layout-store';

describe('useLayoutStore', () => {
  beforeEach(() => {
    useLayoutStore.setState({ requestLayout: 'stacked' });
  });

  it('defaults to stacked', () => {
    expect(useLayoutStore.getState().requestLayout).toBe('stacked');
  });

  it('setRequestLayout updates to side-by-side', () => {
    useLayoutStore.getState().setRequestLayout('side-by-side');
    expect(useLayoutStore.getState().requestLayout).toBe('side-by-side');
  });

  it('setRequestLayout can toggle back to stacked', () => {
    useLayoutStore.getState().setRequestLayout('side-by-side');
    useLayoutStore.getState().setRequestLayout('stacked');
    expect(useLayoutStore.getState().requestLayout).toBe('stacked');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
yarn test layout-store
```

Expected: FAIL — `Cannot find module '../layout-store'`

- [ ] **Step 3: Create the store**

Create `src/stores/layout-store.ts`:

```ts
import { create } from 'zustand';

type RequestLayout = 'stacked' | 'side-by-side';

interface LayoutStore {
  requestLayout: RequestLayout;
  setRequestLayout: (dir: RequestLayout) => void;
}

export const useLayoutStore = create<LayoutStore>()((set) => ({
  requestLayout: 'stacked',
  setRequestLayout: (dir) => set({ requestLayout: dir }),
}));
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
yarn test layout-store
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stores/layout-store.ts src/stores/__tests__/layout-store.test.ts
git commit -m "feat(layout): add useLayoutStore Zustand store"
```

---

## Task 4: Wire layout store into persistence

**Files:**
- Modify: `src/lib/ui-state.ts`
- Modify: `src/App.tsx`

- [ ] **Step 1: Include `layoutDirection` in `scheduleSaveUiState`**

Open `src/lib/ui-state.ts`. Add the import at the top:

```ts
import { useLayoutStore } from '@/stores/layout-store';
```

Then in `scheduleSaveUiState`, after building the `uiState` object, add `layoutDirection`:

The current save block looks like:

```ts
const uiState: UiState = {
  activeMode: isWsMode ? 'workspace' : 'collection',
};
```

Change it to:

```ts
const layoutState = useLayoutStore.getState();
const uiState: UiState = {
  activeMode: isWsMode ? 'workspace' : 'collection',
  layoutDirection: layoutState.requestLayout,
};
```

- [ ] **Step 2: Initialize layout store from restored state in `App.tsx`**

Open `src/App.tsx`. Add the import:

```ts
import { useLayoutStore } from '@/stores/layout-store';
```

Find the block that reads the restored `uiState` (around line 31):

```ts
const uiState = await restoreUiState();
if (uiState?.activeMode === 'workspace' && uiState.workspaceTabs) {
```

Add the layout initialization immediately after `restoreUiState()`:

```ts
const uiState = await restoreUiState();
if (uiState?.layoutDirection) {
  useLayoutStore.getState().setRequestLayout(uiState.layoutDirection);
}
if (uiState?.activeMode === 'workspace' && uiState.workspaceTabs) {
```

- [ ] **Step 3: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/lib/ui-state.ts src/App.tsx
git commit -m "feat(layout): persist requestLayout to ui-state.yml on change and restore on startup"
```

---

## Task 5: Add the toggle button to `StatusBar`

**Files:**
- Modify: `src/components/layout/StatusBar.tsx`
- Test: `src/components/layout/__tests__/StatusBar.test.tsx`

- [ ] **Step 1: Read the existing test to understand the test setup**

Read `src/components/layout/__tests__/StatusBar.test.tsx` to understand the existing test structure before writing new tests.

- [ ] **Step 2: Write failing tests for the toggle button**

Add these test cases to the existing StatusBar test file. Find the existing `describe` block and add inside it:

```ts
import { useLayoutStore } from '@/stores/layout-store';

// inside describe('StatusBar', ...) — add after existing tests:

it('renders "Side by side" button when layout is stacked', () => {
  useLayoutStore.setState({ requestLayout: 'stacked' });
  render(<StatusBar />);
  expect(screen.getByRole('button', { name: /side by side/i })).toBeInTheDocument();
});

it('renders "Stack" button when layout is side-by-side', () => {
  useLayoutStore.setState({ requestLayout: 'side-by-side' });
  render(<StatusBar />);
  expect(screen.getByRole('button', { name: /stack/i })).toBeInTheDocument();
});

it('clicking layout button toggles the store', async () => {
  useLayoutStore.setState({ requestLayout: 'stacked' });
  render(<StatusBar />);
  await userEvent.click(screen.getByRole('button', { name: /side by side/i }));
  expect(useLayoutStore.getState().requestLayout).toBe('side-by-side');
});
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
yarn test StatusBar
```

Expected: new tests fail — button not found.

- [ ] **Step 4: Add the toggle button to `StatusBar`**

Open `src/components/layout/StatusBar.tsx`. Add imports:

```ts
import { PanelBottom, PanelRight } from 'lucide-react';
import { useLayoutStore } from '@/stores/layout-store';
```

Inside `StatusBar`, add the store read:

```ts
const requestLayout = useLayoutStore((s) => s.requestLayout);
const setRequestLayout = useLayoutStore((s) => s.setRequestLayout);
```

In the JSX, add the toggle button before the version string (`{version && ...}`). The current end of the status bar JSX is:

```tsx
{version && <span className='ml-auto text-2xs text-muted-foreground'>{`v${version}`}</span>}
```

Replace it with:

```tsx
<Button
  variant='ghost'
  size='sm'
  className={cn('h-5 px-1.5 text-xs gap-1 ml-auto', requestLayout === 'side-by-side' && 'bg-accent')}
  onClick={() => setRequestLayout(requestLayout === 'stacked' ? 'side-by-side' : 'stacked')}
  title={requestLayout === 'stacked' ? 'Switch to side by side' : 'Switch to stacked'}
  aria-label={requestLayout === 'stacked' ? 'Side by side' : 'Stack'}
>
  {requestLayout === 'stacked' ? (
    <PanelBottom className='h-3.5 w-3.5 text-muted-foreground' />
  ) : (
    <PanelRight className='h-3.5 w-3.5 text-muted-foreground' />
  )}
  {requestLayout === 'stacked' ? 'Side by side' : 'Stack'}
</Button>
{version && <span className='text-2xs text-muted-foreground'>{`v${version}`}</span>}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
yarn test StatusBar
```

Expected: all tests pass.

- [ ] **Step 6: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/layout/StatusBar.tsx src/components/layout/__tests__/StatusBar.test.tsx
git commit -m "feat(status-bar): add layout toggle button"
```

---

## Task 6: Implement side-by-side layout in `RequestPanel`

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

This is the largest change. In side-by-side mode the outer container switches from `flex-col` to `flex-col` still (URL bar at top, then a `flex-row` below), and the drag separator becomes vertical using `clientX`.

- [ ] **Step 1: Add the store read**

At the top of the `RequestPanel` function body, after existing store reads, add:

```ts
const requestLayout = useLayoutStore((s) => s.requestLayout);
```

And add the import at the top of the file:

```ts
import { useLayoutStore } from '@/stores/layout-store';
```

- [ ] **Step 2: Add horizontal split state**

The existing split state uses `requestHeight` (percentage) for the vertical separator. Add a parallel state for the horizontal split width:

After the existing:
```ts
const [requestHeight, setRequestHeight] = useState(55);
const [isDragging, setIsDragging] = useState(false);
```

Add:
```ts
const [requestWidth, setRequestWidth] = useState(50);
```

- [ ] **Step 3: Add the horizontal drag handler**

After the existing `handleSeparatorDown` callback, add:

```ts
const handleVerticalSeparatorDown = useCallback(
  (e: React.PointerEvent) => {
    e.preventDefault();
    setIsDragging(true);
    const container = containerRef.current;
    if (!container) return;

    const startX = e.clientX;
    const startWidth = requestWidth;
    const containerW = container.getBoundingClientRect().width;

    const onMove = (ev: PointerEvent) => {
      const delta = ev.clientX - startX;
      const pct = startWidth + (delta / containerW) * 100;
      setRequestWidth(Math.min(80, Math.max(20, pct)));
    };
    const onUp = () => {
      setIsDragging(false);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  },
  [requestWidth],
);
```

- [ ] **Step 4: Add the keyboard handler for the vertical separator**

After `handleSeparatorKeyDown`, add:

```ts
const handleVerticalSeparatorKeyDown = useCallback((e: React.KeyboardEvent) => {
  if (e.key === 'ArrowLeft') {
    e.preventDefault();
    setRequestWidth((w) => Math.min(80, Math.max(20, w - 5)));
  } else if (e.key === 'ArrowRight') {
    e.preventDefault();
    setRequestWidth((w) => Math.min(80, Math.max(20, w + 5)));
  }
}, []);
```

- [ ] **Step 5: Replace the return JSX with layout-aware rendering**

Find the current `return (` block starting at line 716. The current outer structure is:

```tsx
return (
  <div ref={containerRef} className='flex h-full flex-col overflow-hidden bg-transparent'>
    {/* ── Request area ── */}
    <div
      className={cn(
        'flex flex-col overflow-hidden bg-card/80',
        expandFull ? 'flex-1' : 'h-(--req-h) min-h-[20%] max-h-[80%]',
      )}
      style={expandFull ? undefined : ({ '--req-h': `${requestHeight}%` } as React.CSSProperties)}
    >
      {/* URL bar */}
      ...
      {/* Section tabs */}
      ...
    </div>

    {/* ── Drag separator and response area — hidden on Docs/Settings tabs. ── */}
    {!expandFull && (
      <>
        <div role='separator' ... /> {/* horizontal separator */}
        <div className='flex-1 flex flex-col overflow-hidden bg-card/65 min-h-0'>
          {/* response content */}
        </div>
      </>
    )}
    ...
  </div>
);
```

Replace the entire `return (...)` block with the layout-aware version below. The URL bar JSX and section tabs JSX are unchanged — only the outer wrapper and separator/response area change.

Extract the URL bar and section tabs into a named fragment to avoid duplication. Add these two variables just before `return`:

```tsx
const urlBar = (
  <div className='flex items-center gap-2 border-b border-border/70 px-3 py-2 bg-card/70 backdrop-blur-sm'>
    {/* === PASTE THE EXISTING URL BAR JSX HERE VERBATIM === */}
  </div>
);

const sectionTabs = (
  <div className='flex-1 flex flex-col min-h-0 bg-card/50'>
    <RocketTabBar tabs={tabDefs} rightContent={tabRightContent} />
    <div className='flex-1 overflow-auto p-3 bg-card/65'>
      {/* === PASTE THE EXISTING SECTION TAB CONTENT JSX HERE VERBATIM === */}
    </div>
  </div>
);

const responseArea = sending ? (
  <div className='flex flex-1 flex-col items-center justify-center gap-3'>
    <Loader2 className='h-5 w-5 animate-spin text-primary' />
    <p className='text-sm text-muted-foreground'>Sending request...</p>
  </div>
) : response ? (
  <ResponseBodyViewer response={response} />
) : (
  <div className='flex flex-1 flex-col items-center justify-center gap-3'>
    <RocketLiftOff className='w-24 h-24' />
    <p className='text-sm font-medium text-foreground'>Ready for liftoff</p>
    <p className='text-xs text-muted-foreground'>Send a request to see the response here</p>
    <p className='text-xs text-muted-foreground mt-1'>
      Press{' '}
      <kbd className='rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-2xs'>
        Ctrl+Enter
      </kbd>{' '}
      to send
    </p>
  </div>
);
```

Then the return:

```tsx
if (requestLayout === 'side-by-side') {
  return (
    <div ref={containerRef} className='flex h-full flex-col overflow-hidden bg-transparent'>
      {urlBar}
      <div className='flex flex-1 min-h-0'>
        {/* Request side */}
        <div
          className='flex flex-col overflow-hidden bg-card/80 min-w-[20%] max-w-[80%]'
          style={{ width: `${requestWidth}%` }}
        >
          {sectionTabs}
        </div>

        {/* Vertical separator */}
        {/* biome-ignore lint/a11y/useSemanticElements: drag splitter cannot be an <hr> */}
        <div
          role='separator'
          tabIndex={0}
          aria-orientation='vertical'
          aria-label='Resize request and response panels'
          aria-valuemin={20}
          aria-valuemax={80}
          aria-valuenow={Math.round(requestWidth)}
          onPointerDown={handleVerticalSeparatorDown}
          onKeyDown={handleVerticalSeparatorKeyDown}
          className={cn(
            'w-3 flex items-center justify-center cursor-col-resize select-none border-x transition-colors',
            'focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-1',
            isDragging
              ? 'bg-primary/15 border-primary/50'
              : 'bg-muted/50 border-border/70 hover:bg-accent/70 hover:border-primary/40',
          )}
        >
          <div
            className={cn(
              'rounded-full transition-all',
              isDragging ? 'h-24 w-1.5 bg-primary' : 'h-16 w-1 bg-muted-foreground/40',
            )}
          />
        </div>

        {/* Response side */}
        <div className='flex-1 flex flex-col overflow-hidden bg-card/65 min-w-0'>
          {responseArea}
        </div>
      </div>

      <LoadTestDialog
        open={showLoadTest}
        onOpenChange={setShowLoadTest}
        request={request}
        tabId={tab.id}
      />
      {/* ... keep all existing dialogs (AlertDialog, SaveToCollectionDialog, EnvironmentDialog) */}
    </div>
  );
}

return (
  <div ref={containerRef} className='flex h-full flex-col overflow-hidden bg-transparent'>
    {/* ── Request area ── */}
    <div
      className={cn(
        'flex flex-col overflow-hidden bg-card/80',
        expandFull ? 'flex-1' : 'h-(--req-h) min-h-[20%] max-h-[80%]',
      )}
      style={expandFull ? undefined : ({ '--req-h': `${requestHeight}%` } as React.CSSProperties)}
    >
      {urlBar}
      {sectionTabs}
    </div>

    {/* ── Drag separator and response area — hidden on Docs/Settings tabs. ── */}
    {!expandFull && (
      <>
        {/* biome-ignore lint/a11y/useSemanticElements: drag splitter cannot be an <hr> */}
        <div
          role='separator'
          tabIndex={0}
          aria-orientation='horizontal'
          aria-label='Resize request and response panels'
          aria-valuemin={10}
          aria-valuemax={90}
          aria-valuenow={Math.round(requestHeight)}
          onPointerDown={handleSeparatorDown}
          onKeyDown={handleSeparatorKeyDown}
          className={cn(
            'h-3 flex items-center justify-center cursor-row-resize select-none border-y transition-colors',
            'focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-1',
            isDragging
              ? 'bg-primary/15 border-primary/50'
              : 'bg-muted/50 border-border/70 hover:bg-accent/70 hover:border-primary/40',
          )}
        >
          <div
            className={cn(
              'rounded-full transition-all',
              isDragging ? 'w-24 h-1.5 bg-primary' : 'w-16 h-1 bg-muted-foreground/40',
            )}
          />
        </div>

        <div className='flex-1 flex flex-col overflow-hidden bg-card/65 min-h-0'>
          {responseArea}
        </div>
      </>
    )}

    <LoadTestDialog
      open={showLoadTest}
      onOpenChange={setShowLoadTest}
      request={request}
      tabId={tab.id}
    />
    {/* ... keep all existing dialogs unchanged */}
  </div>
);
```

> **Important:** When doing this step, do NOT inline new JSX from scratch — copy the existing URL bar, section tabs, and response area JSX from the current file verbatim. Only the outer structure changes.

- [ ] **Step 6: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/request/RequestPanel.tsx
git commit -m "feat(request-panel): implement side-by-side layout mode"
```

---

## Task 7: Manual smoke test

- [ ] **Step 1: Start the app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Verify default stacked layout**

Open any request tab. The request panel should be stacked (top/bottom) as before. The status bar should show a "Side by side" button with a `PanelBottom` icon.

- [ ] **Step 3: Toggle to side-by-side**

Click "Side by side" in the status bar. The request panel should switch to left/right layout. The button should now read "Stack" with a `PanelRight` icon and have a highlighted background.

- [ ] **Step 4: Verify the vertical separator is draggable**

Drag the vertical separator between request and response. Both panels should resize accordingly. Release — layout should hold.

- [ ] **Step 5: Verify persistence**

Quit and reopen the app. The layout should restore to side-by-side.

- [ ] **Step 6: Verify it applies globally**

Open two request tabs. Both should show side-by-side. Toggle to stacked — both should switch immediately.

- [ ] **Step 7: Commit if any fixes were needed**

```bash
git add -p
git commit -m "fix(request-panel): <describe fix>"
```

---

## Task 8: Run full checks

- [ ] **Step 1: Run all tests**

```bash
yarn test
```

Expected: all tests pass.

- [ ] **Step 2: Lint**

```bash
yarn check
```

Expected: no errors.

- [ ] **Step 3: Rust check**

```bash
cargo check
```

Expected: no errors.
