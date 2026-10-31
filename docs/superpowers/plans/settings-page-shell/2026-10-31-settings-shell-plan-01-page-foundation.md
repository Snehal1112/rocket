# Settings Shell — Plan 1: Page foundation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 1 of 3.** Previous plan: none. Index: `docs/superpowers/plans/settings-page-shell/00-plan-index.md`.
**Next plan to run: `docs/superpowers/plans/settings-page-shell/2026-10-31-settings-shell-plan-02-shortcuts-proxy-secrets.md`**
**Recommended model: Sonnet.**

**Goal:** Ctrl/Cmd+, opens a global Settings page over the editor area with a section nav, a
Support section and an About section. Open tabs stay mounted underneath and come back unchanged.

**Architecture:** A small Zustand store (`useSettingsStore`) holds `open`, `section` and `locked`.
`SettingsPage` reads it and renders the nav from an ordered list (`SETTINGS_SECTIONS`). A new
`MainArea` component replaces the `<main>` element in `App.tsx`: it keeps its children mounted but
`inert` and draws the page on top when `open` is true. `useKeyboardShortcuts` toggles the page on
Ctrl/Cmd+,.

**Tech Stack:** React 19, TypeScript, Zustand 5, shadcn/ui, lucide-react, `@tauri-apps/plugin-opener`
(`openUrl`), `@tauri-apps/plugin-os`, `@tauri-apps/api/app` (`getVersion`), Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-31-settings-page-shell-design.md`, sections 4.1, 4.2,
4.3 (Ctrl+, and Esc rows), 4.5, 4.7. Prerequisites: none.

## Global Constraints

- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`); icons
  from `lucide-react` only; no inline SVG.
- Zustand: narrow selectors (`useSettingsStore((s) => s.open)`); never destructure the whole store
  in a component.
- Comments are short full sentences ending with a period.
- Support URLs, verbatim: `https://github.com/Snehal1112/rocket/issues` ("Report issues") and
  `https://github.com/Snehal1112/rocket` ("GitHub").
- About shows "Unknown" for any value it cannot read; the rest still render.
- The Settings page is not saved in UI state; it starts closed after a restart.
- Verification per task: `yarn tsc --noEmit`, `yarn check`, the task's `yarn test <path>`.

## Review Focus

- **Esc typed inside a field on the page.** It must not close Settings (a user clearing a field
  with Esc loses their place). Covered: Task 2, test "ignores Esc from an input inside the page".
- **Esc inside a Radix popup (Select list) on the page.** The popup renders in a portal outside the
  page's DOM, so the native listener never sees it. Covered by design; plan 02 Task 3 adds a test
  with the Secret Managers provider Select.
- **Tab focus walking into hidden tabs.** The pane tree is `inert` while Settings is open.
  Covered: Task 3, test "makes the tabs inert while Settings is open".
- **Tab state lost on open.** Request drafts and live sessions would vanish if the pane tree
  remounted. Covered: Task 3, test "keeps the children mounted while Settings opens and closes".
- **About when a Tauri call fails** (no version, no data dir). Each row falls back on its own.
  Covered: Task 2, test "shows Unknown for a value it cannot read".

## File Structure

| File | Change |
|---|---|
| `src/stores/settings-store.ts` | **Create**: open, section, lock state and actions |
| `src/stores/__tests__/settings-store.test.ts` | **Create**: store tests |
| `src/components/settings/SettingsSectionHeader.tsx` | **Create**: section title and description |
| `src/components/settings/sections/SupportSection.tsx` | **Create**: Support links |
| `src/components/settings/sections/AboutSection.tsx` | **Create**: version, data folder, platform |
| `src/components/settings/settings-sections.ts` | **Create**: ordered section list, `resolveSection` |
| `src/components/settings/SettingsPage.tsx` | **Create**: header, nav, panel, Esc |
| `src/components/settings/sections/__tests__/SupportSection.test.tsx` | **Create** |
| `src/components/settings/sections/__tests__/AboutSection.test.tsx` | **Create** |
| `src/components/settings/__tests__/SettingsPage.test.tsx` | **Create** |
| `src/components/layout/MainArea.tsx` | **Create**: `<main>` with the Settings overlay |
| `src/components/layout/__tests__/MainArea.test.tsx` | **Create** |
| `src/App.tsx` | Use `MainArea` for `<main>` (around line 247) |
| `src/hooks/useKeyboardShortcuts.ts` | Ctrl/Cmd+, toggles Settings |
| `src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx` | **Create** |

---

### Task 1: Settings store

**Files:**
- Create: `src/stores/settings-store.ts`
- Test: `src/stores/__tests__/settings-store.test.ts`

**Interfaces:**
- Consumes: `create` from `zustand`.
- Produces (used by every later task and plan):

```ts
export type SettingsSection = 'proxy' | 'secret-managers' | 'ai' | 'support' | 'about';
export const useSettingsStore: UseBoundStore<StoreApi<SettingsState>>;
// SettingsState: open, section (null until picked), locked, openSettings(section?),
// closeSettings(), toggleSettings(), setSection(section), setLocked(locked), reset().
```

- [ ] **Step 1: Write the failing test**

Create `src/stores/__tests__/settings-store.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { useSettingsStore } from '../settings-store';

const state = () => useSettingsStore.getState();

describe('settings store', () => {
  beforeEach(() => {
    state().reset();
  });

  it('starts closed, unlocked and with no section', () => {
    expect(state().open).toBe(false);
    expect(state().locked).toBe(false);
    expect(state().section).toBeNull();
  });

  it('opens at a section and keeps the section when opened again without one', () => {
    state().openSettings('about');
    expect(state().open).toBe(true);
    expect(state().section).toBe('about');
    state().closeSettings();
    state().openSettings();
    expect(state().section).toBe('about');
  });

  it('toggles open and closed', () => {
    state().toggleSettings();
    expect(state().open).toBe(true);
    state().toggleSettings();
    expect(state().open).toBe(false);
  });

  it('changes the section', () => {
    state().openSettings();
    state().setSection('support');
    expect(state().section).toBe('support');
  });

  it('ignores close, toggle, section changes and reopening at another section while locked', () => {
    state().openSettings('ai');
    state().setLocked(true);
    state().closeSettings();
    state().toggleSettings();
    state().setSection('about');
    state().openSettings('proxy');
    expect(state().open).toBe(true);
    expect(state().section).toBe('ai');
    state().setLocked(false);
    state().closeSettings();
    expect(state().open).toBe(false);
  });

  it('reset clears open, section and lock', () => {
    state().openSettings('about');
    state().setLocked(true);
    state().reset();
    expect(state()).toMatchObject({ open: false, section: null, locked: false });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn test src/stores/__tests__/settings-store.test.ts`
Expected: FAIL with `Failed to resolve import "../settings-store"`.

- [ ] **Step 3: Implement `src/stores/settings-store.ts`**

```ts
import { create } from 'zustand';

export type SettingsSection = 'proxy' | 'secret-managers' | 'ai' | 'support' | 'about';

interface SettingsState {
  open: boolean;
  /** The section to show. Null until one is picked; the page then shows its first section. */
  section: SettingsSection | null;
  /** True while a section must not be left, for example during an AI Agent save. */
  locked: boolean;
  openSettings: (section?: SettingsSection) => void;
  closeSettings: () => void;
  toggleSettings: () => void;
  setSection: (section: SettingsSection) => void;
  setLocked: (locked: boolean) => void;
  reset: () => void;
}

const initialState = {
  open: false,
  section: null as SettingsSection | null,
  locked: false,
};

// Global Settings page state. It is not saved, so Settings starts closed after a restart.
export const useSettingsStore = create<SettingsState>()((set, get) => ({
  ...initialState,

  openSettings(section) {
    // A locked page is already open, and it must not move to another section.
    if (get().locked) return;
    set(section ? { open: true, section } : { open: true });
  },

  closeSettings() {
    if (get().locked) return;
    set({ open: false });
  },

  toggleSettings() {
    if (get().locked) return;
    set({ open: !get().open });
  },

  setSection(section) {
    if (get().locked) return;
    set({ section });
  },

  setLocked(locked) {
    set({ locked });
  },

  reset() {
    set({ ...initialState });
  },
}));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn test src/stores/__tests__/settings-store.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/stores/settings-store.ts src/stores/__tests__/settings-store.test.ts
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(settings): add the settings page store`.

---

### Task 2: Settings page, Support and About

**Files:**
- Create: `src/components/settings/SettingsSectionHeader.tsx`
- Create: `src/components/settings/sections/SupportSection.tsx`
- Create: `src/components/settings/sections/AboutSection.tsx`
- Create: `src/components/settings/settings-sections.ts`
- Create: `src/components/settings/SettingsPage.tsx`
- Test: `src/components/settings/sections/__tests__/SupportSection.test.tsx`
- Test: `src/components/settings/sections/__tests__/AboutSection.test.tsx`
- Test: `src/components/settings/__tests__/SettingsPage.test.tsx`

**Interfaces:**
- Consumes: `useSettingsStore`, `SettingsSection` (Task 1); `getAppDataDir` (`@/lib/tauri-api`,
  line 1350); `getVersion` (`@tauri-apps/api/app`); `platform`, `version`, `arch`
  (`@tauri-apps/plugin-os`); `openUrl` (`@tauri-apps/plugin-opener`).
- Produces:

```ts
export function SettingsSectionHeader(props: { title: string; description?: string }): JSX.Element;
export function SupportSection(): JSX.Element;
export const SUPPORT_LINKS: readonly { label: string; url: string; icon: LucideIcon }[];
export function AboutSection(): JSX.Element;
export interface SettingsSectionDef { id; label; icon; Component; wide? }
export const SETTINGS_SECTIONS: SettingsSectionDef[];
export function resolveSection(id: SettingsSection | null, sections?: SettingsSectionDef[]): SettingsSectionDef;
export function SettingsPage(): JSX.Element; // <section aria-label='Settings'>, fills its parent
```

- [ ] **Step 1: Write the failing Support test**

Create `src/components/settings/sections/__tests__/SupportSection.test.tsx`:

```tsx
import { openUrl } from '@tauri-apps/plugin-opener';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SupportSection } from '../SupportSection';

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

describe('SupportSection', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(openUrl).mockResolvedValue(undefined);
  });

  it('has a Support heading', () => {
    render(<SupportSection />);
    expect(screen.getByRole('heading', { name: 'Support' })).toBeInTheDocument();
  });

  it('opens the issues page', async () => {
    render(<SupportSection />);
    await userEvent.click(screen.getByRole('button', { name: /report issues/i }));
    expect(openUrl).toHaveBeenCalledWith('https://github.com/Snehal1112/rocket/issues');
  });

  it('opens the repository', async () => {
    render(<SupportSection />);
    await userEvent.click(screen.getByRole('button', { name: /github/i }));
    expect(openUrl).toHaveBeenCalledWith('https://github.com/Snehal1112/rocket');
  });

  it('shows the URL in a toast when the browser cannot be opened', async () => {
    vi.mocked(openUrl).mockRejectedValue(new Error('denied'));
    render(<SupportSection />);
    await userEvent.click(screen.getByRole('button', { name: /report issues/i }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        expect.stringContaining('https://github.com/Snehal1112/rocket/issues'),
      ),
    );
  });
});
```

- [ ] **Step 2: Write the failing About test**

Create `src/components/settings/sections/__tests__/AboutSection.test.tsx`:

```tsx
import { getVersion } from '@tauri-apps/api/app';
import { arch, platform, version } from '@tauri-apps/plugin-os';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getAppDataDir } from '@/lib/tauri-api';
import { AboutSection } from '../AboutSection';

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn() }));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: vi.fn(), version: vi.fn(), arch: vi.fn() }));
vi.mock('@/lib/tauri-api', () => ({ getAppDataDir: vi.fn() }));

describe('AboutSection', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(getVersion).mockResolvedValue('1.2.3');
    vi.mocked(getAppDataDir).mockResolvedValue('/home/u/.rocket-api');
    vi.mocked(platform).mockReturnValue('linux');
    vi.mocked(version).mockReturnValue('6.1');
    vi.mocked(arch).mockReturnValue('x86_64');
  });

  it('shows the version, the data folder and the platform', async () => {
    render(<AboutSection />);
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
    expect(await screen.findByText('1.2.3')).toBeInTheDocument();
    expect(await screen.findByText('/home/u/.rocket-api')).toBeInTheDocument();
    expect(screen.getByText('linux 6.1 (x86_64)')).toBeInTheDocument();
  });

  it('shows Unknown for a value it cannot read', async () => {
    vi.mocked(getVersion).mockRejectedValue(new Error('no'));
    vi.mocked(platform).mockImplementation(() => {
      throw new Error('no');
    });
    render(<AboutSection />);
    expect(await screen.findByText('/home/u/.rocket-api')).toBeInTheDocument();
    expect(await screen.findAllByText('Unknown')).toHaveLength(2);
  });
});
```

- [ ] **Step 3: Write the failing page test**

Create `src/components/settings/__tests__/SettingsPage.test.tsx`:

```tsx
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSettingsStore } from '@/stores/settings-store';
import { SettingsPage } from '../SettingsPage';

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn().mockResolvedValue('1.2.3') }));
vi.mock('@tauri-apps/plugin-os', () => ({
  platform: () => 'linux',
  version: () => '6.1',
  arch: () => 'x86_64',
}));
vi.mock('@/lib/tauri-api', () => ({ getAppDataDir: vi.fn().mockResolvedValue('/data') }));

const nav = () => screen.getByRole('navigation', { name: 'Settings sections' });

describe('SettingsPage', () => {
  beforeEach(() => {
    useSettingsStore.getState().reset();
    useSettingsStore.getState().openSettings();
  });

  it('lists the sections in order and shows the first one when none is picked', () => {
    render(<SettingsPage />);
    const labels = within(nav())
      .getAllByRole('button')
      .map((b) => b.textContent);
    expect(labels).toEqual(['Support', 'About']);
    expect(screen.getByRole('heading', { name: 'Support' })).toBeInTheDocument();
    expect(within(nav()).getByRole('button', { name: 'Support' })).toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('shows the first section for a section that is not registered', () => {
    useSettingsStore.getState().setSection('proxy');
    render(<SettingsPage />);
    expect(screen.getByRole('heading', { name: 'Support' })).toBeInTheDocument();
  });

  it('switches the panel when a section is clicked', async () => {
    render(<SettingsPage />);
    await userEvent.click(within(nav()).getByRole('button', { name: 'About' }));
    expect(useSettingsStore.getState().section).toBe('about');
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
  });

  it('closes on the Close button', async () => {
    render(<SettingsPage />);
    await userEvent.click(screen.getByRole('button', { name: 'Close settings' }));
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('disables Close and the other sections while locked', () => {
    useSettingsStore.getState().setLocked(true);
    render(<SettingsPage />);
    expect(screen.getByRole('button', { name: 'Close settings' })).toBeDisabled();
    expect(within(nav()).getByRole('button', { name: 'About' })).toBeDisabled();
    expect(within(nav()).getByRole('button', { name: 'Support' })).toBeEnabled();
  });

  it('closes on Esc from the page', () => {
    render(<SettingsPage />);
    fireEvent.keyDown(screen.getByRole('region', { name: 'Settings' }), { key: 'Escape' });
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('ignores Esc from an input inside the page', () => {
    render(<SettingsPage />);
    const input = document.createElement('input');
    screen.getByRole('region', { name: 'Settings' }).appendChild(input);
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(useSettingsStore.getState().open).toBe(true);
  });

  it('ignores Esc from a nested dialog inside the page', () => {
    render(<SettingsPage />);
    const inner = document.createElement('div');
    inner.setAttribute('role', 'dialog');
    screen.getByRole('region', { name: 'Settings' }).appendChild(inner);
    fireEvent.keyDown(inner, { key: 'Escape' });
    expect(useSettingsStore.getState().open).toBe(true);
  });

  it('takes focus when it opens', () => {
    render(<SettingsPage />);
    expect(screen.getByRole('region', { name: 'Settings' })).toHaveFocus();
  });
});
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `yarn test src/components/settings/sections/__tests__ src/components/settings/__tests__/SettingsPage.test.tsx`
Expected: FAIL with `Failed to resolve import "../SupportSection"`, `"../AboutSection"` and
`"../SettingsPage"`.

- [ ] **Step 5: Implement `src/components/settings/SettingsSectionHeader.tsx`**

```tsx
interface SettingsSectionHeaderProps {
  title: string;
  description?: string;
}

// Title and optional description at the top of a Settings section.
export function SettingsSectionHeader({ title, description }: SettingsSectionHeaderProps) {
  return (
    <div className='mb-4 space-y-1'>
      <h2 className='text-base font-semibold'>{title}</h2>
      {description && <p className='text-sm text-muted-foreground'>{description}</p>}
    </div>
  );
}
```

- [ ] **Step 6: Implement `src/components/settings/sections/SupportSection.tsx`**

```tsx
import { openUrl } from '@tauri-apps/plugin-opener';
import { Bug, ExternalLink, GitFork, type LucideIcon } from 'lucide-react';
import { toast } from 'sonner';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { Button } from '@/components/ui/button';

export const SUPPORT_LINKS: readonly { label: string; url: string; icon: LucideIcon }[] = [
  { label: 'Report issues', url: 'https://github.com/Snehal1112/rocket/issues', icon: Bug },
  { label: 'GitHub', url: 'https://github.com/Snehal1112/rocket', icon: GitFork },
];

async function openLink(url: string) {
  try {
    await openUrl(url);
  } catch {
    // The URL is in the message so the user can copy it.
    toast.error(`Could not open the browser. Go to ${url}`);
  }
}

export function SupportSection() {
  return (
    <div>
      <SettingsSectionHeader title='Support' description='Get help and report problems.' />
      <div className='flex flex-col items-start gap-2'>
        {SUPPORT_LINKS.map(({ label, url, icon: Icon }) => (
          <Button
            key={url}
            variant='link'
            className='h-auto gap-2 p-0'
            onClick={() => void openLink(url)}
          >
            <Icon className='h-4 w-4' aria-hidden='true' />
            {label}
            <ExternalLink className='h-3 w-3' aria-hidden='true' />
          </Button>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Implement `src/components/settings/sections/AboutSection.tsx`**

```tsx
import { getVersion } from '@tauri-apps/api/app';
import { arch, platform, version } from '@tauri-apps/plugin-os';
import { useEffect, useState } from 'react';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { getAppDataDir } from '@/lib/tauri-api';

const UNKNOWN = 'Unknown';

function readPlatform(): string {
  try {
    return `${platform()} ${version()} (${arch()})`;
  } catch {
    return UNKNOWN;
  }
}

export function AboutSection() {
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [platformText] = useState(readPlatform);

  useEffect(() => {
    let disposed = false;
    // Each value falls back on its own, so one failure does not hide the others.
    getVersion().then(
      (v) => !disposed && setAppVersion(v),
      () => !disposed && setAppVersion(UNKNOWN),
    );
    getAppDataDir().then(
      (d) => !disposed && setDataDir(d),
      () => !disposed && setDataDir(UNKNOWN),
    );
    return () => {
      disposed = true;
    };
  }, []);

  const rows: [string, string | null][] = [
    ['Version', appVersion],
    ['Data folder', dataDir],
    ['Platform', platformText],
  ];

  return (
    <div>
      <SettingsSectionHeader title='About' description='Rocket, an API client.' />
      <dl className='grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2 text-sm'>
        {rows.map(([label, value]) => (
          <div key={label} className='contents'>
            <dt className='text-muted-foreground'>{label}</dt>
            <dd className='select-text break-all'>{value ?? '…'}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
```

- [ ] **Step 8: Implement `src/components/settings/settings-sections.ts`**

```ts
import { Info, LifeBuoy, type LucideIcon } from 'lucide-react';
import type { ComponentType } from 'react';
import { AboutSection } from '@/components/settings/sections/AboutSection';
import { SupportSection } from '@/components/settings/sections/SupportSection';
import type { SettingsSection } from '@/stores/settings-store';

export interface SettingsSectionDef {
  id: SettingsSection;
  label: string;
  icon: LucideIcon;
  Component: ComponentType;
  /** A wider panel, for sections with lists and wide forms. */
  wide?: boolean;
}

// Bruno's order. Later sections go in at their Bruno position (spec section 4.5).
export const SETTINGS_SECTIONS: SettingsSectionDef[] = [
  { id: 'support', label: 'Support', icon: LifeBuoy, Component: SupportSection },
  { id: 'about', label: 'About', icon: Info, Component: AboutSection },
];

// An unknown or missing id falls back to the first section.
export function resolveSection(
  id: SettingsSection | null,
  sections: SettingsSectionDef[] = SETTINGS_SECTIONS,
): SettingsSectionDef {
  return sections.find((s) => s.id === id) ?? sections[0];
}
```

- [ ] **Step 9: Implement `src/components/settings/SettingsPage.tsx`**

```tsx
import { X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { resolveSection, SETTINGS_SECTIONS } from '@/components/settings/settings-sections';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { useSettingsStore } from '@/stores/settings-store';

// Esc from these keeps Settings open, so typing and nested popups are not interrupted.
const ESC_IGNORED_FROM =
  'input, textarea, select, [contenteditable="true"], [role="combobox"], [role="listbox"], [role="menu"], [role="dialog"], [role="alertdialog"]';

export function SettingsPage() {
  const sectionId = useSettingsStore((s) => s.section);
  const locked = useSettingsStore((s) => s.locked);
  const setSection = useSettingsStore((s) => s.setSection);
  const closeSettings = useSettingsStore((s) => s.closeSettings);
  const rootRef = useRef<HTMLElement>(null);
  const active = resolveSection(sectionId);
  const ActiveSection = active.Component;

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    root.focus();
    // A native listener: Radix popups render in portals outside this subtree, so their Esc stays theirs.
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      if (e.target instanceof Element && e.target.closest(ESC_IGNORED_FROM)) return;
      e.preventDefault();
      useSettingsStore.getState().closeSettings();
    };
    root.addEventListener('keydown', onKeyDown);
    return () => root.removeEventListener('keydown', onKeyDown);
  }, []);

  return (
    <section
      ref={rootRef}
      tabIndex={-1}
      aria-label='Settings'
      className='flex h-full flex-col bg-background outline-none'
    >
      <header className='flex h-10 shrink-0 items-center justify-between border-b px-4'>
        <h1 className='text-sm font-semibold'>Settings</h1>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Close settings'
          disabled={locked}
          onClick={closeSettings}
        >
          <X className='h-4 w-4' aria-hidden='true' />
        </Button>
      </header>
      <div className='flex min-h-0 flex-1'>
        <nav aria-label='Settings sections' className='w-48 shrink-0 space-y-0.5 border-r p-2'>
          {SETTINGS_SECTIONS.map((s) => {
            const Icon = s.icon;
            const isActive = s.id === active.id;
            return (
              <Button
                key={s.id}
                variant='ghost'
                size='sm'
                aria-current={isActive ? 'page' : undefined}
                disabled={locked && !isActive}
                onClick={() => setSection(s.id)}
                className={cn(
                  'w-full justify-start gap-2 font-normal',
                  isActive && 'bg-accent text-accent-foreground',
                )}
              >
                <Icon className='h-4 w-4' aria-hidden='true' />
                {s.label}
              </Button>
            );
          })}
        </nav>
        <div className='min-w-0 flex-1 overflow-y-auto'>
          <div className={cn('p-6', active.wide ? 'max-w-3xl' : 'max-w-xl')}>
            <ActiveSection key={active.id} />
          </div>
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 10: Run the tests to verify they pass**

Run: `yarn test src/components/settings/sections/__tests__ src/components/settings/__tests__/SettingsPage.test.tsx`
Expected: PASS (4 Support, 2 About, 9 SettingsPage tests).

- [ ] **Step 11: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors. If Biome flags `tabIndex` on `<section>`, keep `tabIndex={-1}` (it is
focusable by script only) and add
`// biome-ignore lint/a11y/noNoninteractiveTabindex: focused by script so Esc and screen readers start on the page.`
on the line above the opening tag.

- [ ] **Step 12: Commit**

```bash
git add src/components/settings/SettingsSectionHeader.tsx \
  src/components/settings/settings-sections.ts \
  src/components/settings/SettingsPage.tsx \
  src/components/settings/sections/SupportSection.tsx \
  src/components/settings/sections/AboutSection.tsx \
  src/components/settings/sections/__tests__/SupportSection.test.tsx \
  src/components/settings/sections/__tests__/AboutSection.test.tsx \
  src/components/settings/__tests__/SettingsPage.test.tsx
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(settings): add the settings page with Support and About`.

---

### Task 3: Draw Settings over the editor area and open it with Ctrl/Cmd+,

**Files:**
- Create: `src/components/layout/MainArea.tsx`
- Modify: `src/App.tsx` (the `<main>` element, around lines 247-254; imports at the top)
- Modify: `src/hooks/useKeyboardShortcuts.ts` (top of the `handler`, after the `if (!mod) return;` line, about line 24)
- Test: `src/components/layout/__tests__/MainArea.test.tsx`
- Test: `src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx`

**Interfaces:**
- Consumes: `useSettingsStore` (Task 1), `SettingsPage` (Task 2).
- Produces: `export function MainArea({ children }: { children: ReactNode }): JSX.Element;`

- [ ] **Step 1: Write the failing MainArea test**

Create `src/components/layout/__tests__/MainArea.test.tsx`:

```tsx
import { act, render, screen } from '@testing-library/react';
import { useEffect } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSettingsStore } from '@/stores/settings-store';
import { MainArea } from '../MainArea';

vi.mock('@/components/settings/SettingsPage', () => ({
  SettingsPage: () => <section aria-label='Settings' />,
}));

let mounts = 0;
function Child() {
  useEffect(() => {
    mounts += 1;
  }, []);
  return <p>tab content</p>;
}

describe('MainArea', () => {
  beforeEach(() => {
    mounts = 0;
    useSettingsStore.getState().reset();
  });

  it('shows only its children while Settings is closed', () => {
    render(
      <MainArea>
        <Child />
      </MainArea>,
    );
    expect(screen.getByText('tab content')).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Settings' })).toBeNull();
  });

  it('keeps the children mounted while Settings opens and closes', () => {
    render(
      <MainArea>
        <Child />
      </MainArea>,
    );
    act(() => useSettingsStore.getState().openSettings());
    expect(screen.getByRole('region', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByText('tab content')).toBeInTheDocument();
    act(() => useSettingsStore.getState().closeSettings());
    expect(screen.queryByRole('region', { name: 'Settings' })).toBeNull();
    expect(mounts).toBe(1);
  });

  it('makes the tabs inert while Settings is open', () => {
    render(
      <MainArea>
        <Child />
      </MainArea>,
    );
    const wrapper = screen.getByText('tab content').parentElement as HTMLElement;
    expect(wrapper).not.toHaveAttribute('inert');
    act(() => useSettingsStore.getState().openSettings());
    expect(wrapper).toHaveAttribute('inert');
  });
});
```

- [ ] **Step 2: Write the failing shortcut test**

Create `src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { useSettingsStore } from '@/stores/settings-store';
import { useKeyboardShortcuts } from '../useKeyboardShortcuts';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn(), cancelAutoSave: vi.fn() }));
vi.mock('@/lib/execute-request', () => ({ sendRequest: vi.fn() }));

const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

function press(key: string) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, ctrlKey: true }));
}

describe('Settings shortcuts', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useSettingsStore.getState().reset();
  });

  it('toggles Settings on Ctrl+,', () => {
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    press(',');
    expect(useSettingsStore.getState().open).toBe(true);
    press(',');
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('does not toggle on a plain comma', () => {
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    window.dispatchEvent(new KeyboardEvent('keydown', { key: ',' }));
    expect(useSettingsStore.getState().open).toBe(false);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test src/components/layout/__tests__/MainArea.test.tsx src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx`
Expected: FAIL. MainArea with `Failed to resolve import "../MainArea"`; the shortcut test with
`expected false to be true` in "toggles Settings on Ctrl+,".

- [ ] **Step 4: Implement `src/components/layout/MainArea.tsx`**

```tsx
import type { ReactNode } from 'react';
import { SettingsPage } from '@/components/settings/SettingsPage';
import { useSettingsStore } from '@/stores/settings-store';

// The editor area. Settings opens on top of it, so open tabs stay mounted and come back unchanged.
export function MainArea({ children }: { children: ReactNode }) {
  const settingsOpen = useSettingsStore((s) => s.open);
  return (
    <main className='relative flex-1 flex flex-col min-w-0 overflow-hidden'>
      {/* Inert while covered, so Tab cannot move focus into the hidden tabs. */}
      <div inert={settingsOpen} className='flex min-h-0 flex-1 flex-col'>
        {children}
      </div>
      {settingsOpen && (
        <div className='absolute inset-0 z-20'>
          <SettingsPage />
        </div>
      )}
    </main>
  );
}
```

- [ ] **Step 5: Use `MainArea` in `src/App.tsx`**

Add the import next to the other layout imports (keep Biome's order; `yarn lint` fixes it):

```tsx
import { MainArea } from '@/components/layout/MainArea';
```

Replace (around lines 247-254):

```tsx
        <main className='flex-1 flex flex-col min-w-0 overflow-hidden'>
          <WorkspaceToolbar />
          <div className='flex-1 min-h-0 overflow-hidden'>
            <ErrorBoundary>
              <PaneRenderer node={root} />
            </ErrorBoundary>
          </div>
        </main>
```

with:

```tsx
        <MainArea>
          <WorkspaceToolbar />
          <div className='flex-1 min-h-0 overflow-hidden'>
            <ErrorBoundary>
              <PaneRenderer node={root} />
            </ErrorBoundary>
          </div>
        </MainArea>
```

- [ ] **Step 6: Add Ctrl/Cmd+, to `src/hooks/useKeyboardShortcuts.ts`**

Add the import:

```ts
import { useSettingsStore } from '@/stores/settings-store';
```

In `handler`, directly after `if (!mod) return;` and before `const store = usePaneStore.getState();`,
insert:

```ts
      // Cmd/Ctrl+, — open or close Settings.
      if (e.key === ',') {
        e.preventDefault();
        useSettingsStore.getState().toggleSettings();
        return;
      }
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `yarn test src/components/layout/__tests__/MainArea.test.tsx src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx src/hooks/__tests__/useKeyboardShortcuts-close.test.tsx src/hooks/__tests__/useKeyboardShortcuts-flow-run.test.tsx`
Expected: PASS (3 MainArea, 2 settings-shortcut tests; the existing close and flow-run tests still
pass).

- [ ] **Step 8: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add src/components/layout/MainArea.tsx src/components/layout/__tests__/MainArea.test.tsx \
  src/App.tsx src/hooks/useKeyboardShortcuts.ts \
  src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(settings): open settings over the editor with Ctrl+,`.

---

## End of Plan 1. Verify

- `yarn tsc --noEmit` and `yarn check` pass.
- `yarn test src/stores/__tests__/settings-store.test.ts src/components/settings src/components/layout/__tests__/MainArea.test.tsx src/hooks/__tests__` passes.
- Manual (`yarn tauri dev`): Ctrl+, opens Settings with Support and About; an unsaved request
  draft is unchanged after Ctrl+, twice; Esc closes; the Support links open the browser.

Next: `2026-10-31-settings-shell-plan-02-shortcuts-proxy-secrets.md`.

## Self-Review

- Spec coverage: 4.1 (Task 3, MainArea and App), 4.2 (Task 1), 4.3 Ctrl+, and Esc rows (Tasks 3
  and 2), 4.5 (Task 2), 4.7 (Task 2), 5 About and Support errors (Task 2). The rest of 4.3, 4.4,
  4.6 and 4.8 are in plans 02 and 03.
- Names used by later plans: `useSettingsStore`, `SettingsSection`, `SETTINGS_SECTIONS`,
  `SettingsSectionDef`, `SettingsSectionHeader`, `MainArea`.
- Not covered here: the title-bar gear (plan 02, Task 3), shortcut suppression and `openTab`
  closing Settings (plan 02, Task 1).
