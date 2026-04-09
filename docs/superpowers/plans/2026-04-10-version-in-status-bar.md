# Version in Status Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display the Tauri app version as a plain `v0.4.0` text label in the bottom-right corner of the status bar.

**Architecture:** Call `getVersion()` from `@tauri-apps/api/app` on mount inside `StatusBar`, store the result in local state, and render it with `ml-auto` to push it to the far right of the existing flex row.

**Tech Stack:** React, TypeScript, `@tauri-apps/api/app`, Vitest + Testing Library

---

## File Map

| Action | File |
|--------|------|
| Modify | `src/components/layout/StatusBar.tsx` |
| Create | `src/components/layout/__tests__/StatusBar.test.tsx` |

---

### Task 1: Write the failing test

**Files:**
- Create: `src/components/layout/__tests__/StatusBar.test.tsx`

- [ ] **Step 1: Create the test file**

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { StatusBar } from '../StatusBar';

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.4.0'),
}));

vi.mock('@/hooks/useTheme', () => ({
  useTheme: () => ({ isDark: false, toggleTheme: vi.fn() }),
}));

vi.mock('@/stores/console-store', () => ({
  useConsoleStore: (selector: (s: { entries: unknown[] }) => unknown) =>
    selector({ entries: [] }),
}));

describe('StatusBar', () => {
  it('displays the app version in the bottom-right corner', async () => {
    render(<StatusBar />);
    await waitFor(() => {
      expect(screen.getByText('v0.4.0')).toBeDefined();
    });
  });
});
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
yarn test src/components/layout/__tests__/StatusBar.test.tsx
```

Expected: FAIL — `Unable to find an element with the text: v0.4.0`

---

### Task 2: Implement the feature

**Files:**
- Modify: `src/components/layout/StatusBar.tsx`

- [ ] **Step 1: Update StatusBar.tsx with the version display**

Replace the full file with:

```tsx
import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { Moon, Sun, Terminal } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/hooks/useTheme';
import { cn } from '@/lib/utils';
import { useConsoleStore } from '@/stores/console-store';

interface StatusBarProps {
  isConsoleOpen?: boolean;
  onConsoleToggle?: () => void;
}

export function StatusBar({ isConsoleOpen, onConsoleToggle }: StatusBarProps) {
  const entryCount = useConsoleStore((s) => s.entries.length);
  const { isDark, toggleTheme } = useTheme();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  return (
    <div className='h-7 border-t border-border/70 bg-card/50 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0'>
      <Button
        variant='ghost'
        size='icon'
        onClick={toggleTheme}
        className='h-5 w-5'
        title={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
      >
        {isDark ? (
          <Sun className='h-3.5 w-3.5 text-muted-foreground' />
        ) : (
          <Moon className='h-3.5 w-3.5 text-muted-foreground' />
        )}
      </Button>
      <Button
        variant='ghost'
        size='sm'
        className={cn('h-5 px-1.5 text-xs gap-1', isConsoleOpen && 'bg-accent')}
        onClick={onConsoleToggle}
        aria-label='Toggle Console'
      >
        <Terminal className='h-3.5 w-3.5 text-muted-foreground' />
        Console
        {entryCount > 0 && (
          <span className='text-2xs px-1 rounded-full bg-muted text-muted-foreground'>
            {entryCount}
          </span>
        )}
      </Button>
      {version && (
        <span className='ml-auto text-2xs text-muted-foreground'>{`v${version}`}</span>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Run the test to confirm it passes**

```bash
yarn test src/components/layout/__tests__/StatusBar.test.tsx
```

Expected: PASS

---

### Task 3: Validate and commit

- [ ] **Step 1: Run all frontend tests**

```bash
yarn test
```

Expected: All tests pass.

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: No errors.

- [ ] **Step 3: Lint check**

```bash
yarn check
```

Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/StatusBar.tsx src/components/layout/__tests__/StatusBar.test.tsx
git commit -m "feat(status-bar): display app version in bottom-right corner"
```
