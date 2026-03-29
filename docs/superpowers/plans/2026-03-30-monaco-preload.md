# Monaco Editor Preload Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the cold-load delay for the Monaco editor by preloading its chunk in the background on app startup and replacing the plain text loading state with a styled skeleton.

**Architecture:** A `requestIdleCallback` effect in `App.tsx` fires the dynamic import for `MonacoWrapper` immediately after the app shell renders, populating the module cache before the user opens an editor. A new `EditorSkeleton` component renders a pulsing fake editor (matching Monaco's exact background colors) in place of the "Loading editor..." text at all three callsites.

**Tech Stack:** React 19, TypeScript 5.8, TailwindCSS v4, Tauri 2. No new dependencies.

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `src/components/editor/EditorSkeleton.tsx` | Animated fake-editor skeleton shown while Monaco loads |
| Modify | `src/App.tsx` | Add `requestIdleCallback` preload effect |
| Modify | `src/components/editor/MonacoWrapper.tsx` | Swap text `loading` prop for `<EditorSkeleton>` |
| Modify | `src/components/request/BodyEditor.tsx` | Swap Suspense text fallback for `<EditorSkeleton>` |
| Modify | `src/components/response/ResponseBodyViewer.tsx` | Swap both Suspense text fallbacks for `<EditorSkeleton>` |

---

### Task 1: Create EditorSkeleton component

**Files:**
- Create: `src/components/editor/EditorSkeleton.tsx`

- [ ] **Step 1: Create the file with this exact content**

```tsx
import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';

// Tracks dark mode by observing the html element class changes.
function useIsDark(): boolean {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  );

  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });
    return () => observer.disconnect();
  }, []);

  return isDark;
}

// Widths for the fake code lines — varying to feel like real code.
const LINE_WIDTHS = ['w-3/4', 'w-1/2', 'w-5/6', 'w-2/5', 'w-3/5', 'w-1/3', 'w-4/5'];

// Placeholder shown while Monaco loads — matches Monaco's background exactly.
export function EditorSkeleton() {
  const isDark = useIsDark();

  return (
    <div
      className={cn(
        'flex h-full w-full overflow-hidden font-mono text-xs',
        isDark ? 'bg-[#1f1f1f]' : 'bg-white',
      )}
      aria-hidden="true"
    >
      {/* Line numbers column. */}
      <div
        className={cn(
          'flex w-10 shrink-0 flex-col gap-3 px-2 pt-3',
          isDark ? 'border-r border-[#333]' : 'border-r border-[#e4e4e4]',
        )}
      >
        {[0, 1, 2, 3].map((n) => (
          <div
            key={n}
            className="h-2 w-6 animate-pulse rounded-sm"
            style={{ background: isDark ? '#444' : '#ddd' }}
          />
        ))}
      </div>

      {/* Code content area with shimmer lines. */}
      <div className="flex flex-1 animate-pulse flex-col gap-3 p-3">
        {LINE_WIDTHS.map((width, i) => (
          <div
            key={i}
            className={cn('h-2 rounded-sm', width)}
            style={{ background: isDark ? '#2d2d2d' : '#ececec' }}
          />
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Run TypeScript check**

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/editor/EditorSkeleton.tsx
git commit -m "feat(editor): add EditorSkeleton loading placeholder"
```

---

### Task 2: Wire EditorSkeleton into all three loading callsites

**Files:**
- Modify: `src/components/editor/MonacoWrapper.tsx`
- Modify: `src/components/request/BodyEditor.tsx`
- Modify: `src/components/response/ResponseBodyViewer.tsx`

- [ ] **Step 1: Update MonacoWrapper.tsx**

The current `loading` prop (lines 41–45) is:
```tsx
loading={
  <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
    Loading editor...
  </div>
}
```

Replace with:
```tsx
loading={<EditorSkeleton />}
```

Also add the import at the top of the file (after the existing imports):
```tsx
import { EditorSkeleton } from './EditorSkeleton';
```

The complete updated `MonacoWrapper.tsx`:
```tsx
import Editor, { type OnMount } from '@monaco-editor/react';
import { BASE_EDITOR_OPTIONS, READONLY_OPTIONS, detectLanguage } from './monaco-config';
import { useMonacoTheme } from './useMonacoTheme';
import { EditorSkeleton } from './EditorSkeleton';

interface MonacoWrapperProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;
  bodyMode?: string;
  contentType?: string;
  readOnly?: boolean;
  height?: string;
}

export function MonacoWrapper({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
}: MonacoWrapperProps) {
  const { themeName, defineThemes } = useMonacoTheme();
  const resolvedLanguage = language ?? detectLanguage(bodyMode, contentType);
  const options = readOnly ? READONLY_OPTIONS : BASE_EDITOR_OPTIONS;

  const handleMount: OnMount = (_editor, monaco) => {
    defineThemes(monaco);
  };

  return (
    <Editor
      height={height}
      language={resolvedLanguage}
      value={value}
      onChange={(val) => onChange?.(val ?? '')}
      onMount={handleMount}
      theme={themeName}
      options={options}
      loading={<EditorSkeleton />}
    />
  );
}
```

- [ ] **Step 2: Update BodyEditor.tsx**

Add the import after the existing imports at the top:
```tsx
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
```

Replace the Suspense fallback (lines 62–67):
```tsx
// Before:
<Suspense
  fallback={
    <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
      Loading editor...
    </div>
  }
>

// After:
<Suspense fallback={<EditorSkeleton />}>
```

- [ ] **Step 3: Update ResponseBodyViewer.tsx**

Add the import after the existing imports:
```tsx
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
```

There are two Suspense blocks with the same fallback text (around lines 183–189 and 206–212). Replace both:
```tsx
// Before (both occurrences):
<Suspense
  fallback={
    <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
      Loading editor...
    </div>
  }
>

// After (both occurrences):
<Suspense fallback={<EditorSkeleton />}>
```

- [ ] **Step 4: Run TypeScript check**

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add src/components/editor/MonacoWrapper.tsx \
        src/components/request/BodyEditor.tsx \
        src/components/response/ResponseBodyViewer.tsx
git commit -m "feat(editor): replace loading text with EditorSkeleton at all callsites"
```

---

### Task 3: Add preload effect to App.tsx

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add the preload effect**

In `src/App.tsx`, the imports already include `useState, useEffect` from React (line 14). Add the preload `useEffect` block after the existing `useEffect` for the Linux OS class (lines 54–58), before the `return` statement:

```tsx
// Preload Monaco in the background after the app shell renders.
useEffect(() => {
  const id = requestIdleCallback(
    () => { void import('@/components/editor/MonacoWrapper'); },
    { timeout: 2000 },
  );
  return () => cancelIdleCallback(id);
}, []);
```

The complete `App.tsx` after the change — only the relevant section shown (add between the linux OS effect and the return):

```tsx
  useEffect(() => {
    if (osType() === 'linux') {
      document.documentElement.classList.add('linux');
    }
  }, []);

  // Preload Monaco in the background after the app shell renders.
  useEffect(() => {
    const id = requestIdleCallback(
      () => { void import('@/components/editor/MonacoWrapper'); },
      { timeout: 2000 },
    );
    return () => cancelIdleCallback(id);
  }, []);

  return (
```

- [ ] **Step 2: Run TypeScript check**

Run: `yarn tsc --noEmit`
Expected: no errors. If TypeScript reports `requestIdleCallback` not found, add this declaration directly above the `App` function in `App.tsx`:

```tsx
declare function requestIdleCallback(cb: IdleRequestCallback, opts?: IdleRequestOptions): number;
declare function cancelIdleCallback(id: number): void;
```

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat(editor): preload Monaco chunk via requestIdleCallback on app startup"
```
