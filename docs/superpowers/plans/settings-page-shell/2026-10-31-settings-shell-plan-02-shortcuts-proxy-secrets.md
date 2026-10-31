# Settings Shell — Plan 2: Shortcuts, Proxy and Secret Managers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 2 of 3.** Previous plan: `docs/superpowers/plans/settings-page-shell/2026-10-31-settings-shell-plan-01-page-foundation.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/settings-page-shell/2026-10-31-settings-shell-plan-03-ai-agents-and-cleanup.md`**
**Recommended model: Sonnet.**

**Goal:** While Settings is open, tab shortcuts cannot act on the hidden tabs and opening a tab
closes Settings. Proxy and Secret Managers become Settings sections, and the title-bar gear opens
Settings.

**Architecture:** `useKeyboardShortcuts` checks `useSettingsStore.getState().open` before the tab
shortcuts. `pane-store` `openTab` and `openCollectionTab` call `closeSettings()` (a no-op while
locked). `ProxySettingsDialog` and `SecretManagerConnectionsDialog` are moved with `git mv` into
`src/components/settings/sections/` and lose their `<Dialog>` wrapper and `open` props; their
state resets on unmount. Each is registered in `SETTINGS_SECTIONS`. `TitleBar` drops the globe
button and its gear calls `toggleSettings()`.

**Tech Stack:** React 19, TypeScript, Zustand 5, TanStack Query, shadcn/ui, lucide-react, Vitest +
Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-31-settings-page-shell-design.md`, sections 4.3
(`openTab` row), 4.4, 4.6 (Proxy and Secret Managers rows), 4.8 (title bar). Prerequisites from
plan 01: `useSettingsStore`, `SETTINGS_SECTIONS`, `SettingsSectionDef`, `SettingsSectionHeader`,
`SettingsPage`, `MainArea`.

## Global Constraints

- shadcn/ui primitives only; lucide-react icons only; narrow Zustand selectors.
- Moved sections keep their state, queries, mutations, validation and messages. Only the dialog
  wrapper, the `open`/`onOpenChange` props and the reset-on-close effect go.
- Proxy: after a successful Save the section stays, shows the existing "Proxy settings saved" toast
  and loads the stored setting again. A load failure keeps Save disabled.
- Comments are short full sentences ending with a period.
- Verification per task: `yarn tsc --noEmit`, `yarn check`, the task's `yarn test <path>`.

## Review Focus

- **Ctrl+W with Settings open.** It must close Settings, not the hidden tab (losing a dirty draft
  the user cannot see). Covered: Task 1, "closes Settings instead of the tab on Ctrl+W".
- **Ctrl+W, Ctrl+S or a sidebar click while an AI Agent save locks Settings.** Nothing closes and
  no tab closes. Covered: Task 1, "keeps Settings and the tab on Ctrl+W while locked" and "keeps
  Settings open when a tab opens while locked".
- **Proxy Save, then the stored password state.** After saving a typed password the field must be
  empty and the "password is saved" hint must come from the reloaded setting, not from local state.
  Covered: Task 2, "stays on the section and reloads the saved setting after Save".
- **Proxy load failed, then the user wants to retry.** The dialog said "close and try again"; a
  section needs its own retry. Covered: Task 2, "retries a failed load on Reload".
- **Esc inside the provider Select of Secret Managers.** It must close the Select only, not
  Settings. Covered: Task 3, "keeps Settings open when Esc closes the provider list".

## File Structure

| File | Change |
|---|---|
| `src/hooks/useKeyboardShortcuts.ts` | Tab shortcuts off while Settings is open; Ctrl+W closes Settings |
| `src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx` | Add suppression tests |
| `src/stores/pane-store.ts` | `openTab` (line 467) and `openCollectionTab` (line 1576) close Settings |
| `src/stores/__tests__/pane-store-settings.test.ts` | **Create** |
| `src/components/settings/ProxySettingsDialog.tsx` → `src/components/settings/sections/ProxySection.tsx` | **Move** and rewrite as a section |
| `src/components/settings/__tests__/ProxySettingsDialog.test.tsx` → `src/components/settings/sections/__tests__/ProxySection.test.tsx` | **Move**, add three tests |
| `src/components/settings/SecretManagerConnectionsDialog.tsx` → `src/components/settings/sections/SecretManagersSection.tsx` | **Move**, drop the dialog wrapper |
| `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx` → `src/components/settings/sections/__tests__/SecretManagersSection.test.tsx` | **Move**, add the Esc test |
| `src/components/settings/settings-sections.ts` | Register Proxy and Secret Managers |
| `src/components/title-bar/TitleBar.tsx` | Drop the globe button (Task 2); gear opens Settings (Task 3) |
| `src/components/title-bar/__tests__/TitleBar.test.tsx` | **Create** (Task 2), extend (Task 3) |

---

### Task 1: Tab shortcuts and tab opening while Settings is open

**Files:**
- Modify: `src/hooks/useKeyboardShortcuts.ts` (after the Ctrl+, block added in plan 01)
- Modify: `src/stores/pane-store.ts:467` (`openTab`) and `:1576` (`openCollectionTab`)
- Test: `src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx`
- Test: `src/stores/__tests__/pane-store-settings.test.ts`

**Interfaces:**
- Consumes: `useSettingsStore` (plan 01); `usePaneStore` `openTab`, `openEphemeralTab`,
  `openCollectionTab`, `closeTab`, `reset`.
- Produces: no new names. Behavior: `openTab` and `openCollectionTab` (when it finds a tab) call
  `useSettingsStore.getState().closeSettings()` first.

- [ ] **Step 1: Write the failing shortcut tests**

In `src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx`, add `afterEach` to the vitest
import and `import { sendRequest } from '@/lib/execute-request';`, then append inside the
`describe('Settings shortcuts', …)` block:

```tsx
  describe('while Settings is open', () => {
    const realCloseTab = usePaneStore.getState().closeTab;
    let closeTab: ReturnType<typeof vi.fn>;
    let saveEvents: string[];
    const onSave = (e: Event) => saveEvents.push(e.type);

    beforeEach(() => {
      vi.mocked(sendRequest).mockClear();
      usePaneStore.getState().openEphemeralTab();
      closeTab = vi.fn();
      usePaneStore.setState({ closeTab } as never);
      saveEvents = [];
      window.addEventListener('rocket:save-to-collection', onSave);
      window.addEventListener('rocket:save-draft', onSave);
      useSettingsStore.getState().openSettings();
    });
    afterEach(() => {
      window.removeEventListener('rocket:save-to-collection', onSave);
      window.removeEventListener('rocket:save-draft', onSave);
      usePaneStore.setState({ closeTab: realCloseTab });
    });

    it('does not send the hidden request on Ctrl+Enter', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('Enter');
      expect(sendRequest).not.toHaveBeenCalled();
    });

    it('does not save the hidden tab on Ctrl+S', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('s');
      expect(saveEvents).toEqual([]);
    });

    it('closes Settings instead of the tab on Ctrl+W', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('w');
      expect(useSettingsStore.getState().open).toBe(false);
      expect(closeTab).not.toHaveBeenCalled();
    });

    it('keeps Settings and the tab on Ctrl+W while locked', () => {
      useSettingsStore.getState().setLocked(true);
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('w');
      expect(useSettingsStore.getState().open).toBe(true);
      expect(closeTab).not.toHaveBeenCalled();
    });

    it('sends the request again once Settings is closed', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      useSettingsStore.getState().closeSettings();
      press('Enter');
      expect(sendRequest).toHaveBeenCalledTimes(1);
    });
  });
```

- [ ] **Step 2: Write the failing pane-store test**

Create `src/stores/__tests__/pane-store-settings.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CollectionTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';
import { useSettingsStore } from '../settings-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn(), cancelAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const collectionTab: CollectionTab = {
  id: 'collection-demo',
  title: 'demo',
  isDirty: false,
  tabType: 'collection',
  collectionName: 'demo',
};

describe('pane store and Settings', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useSettingsStore.getState().reset();
  });

  it('closes Settings when a tab opens', () => {
    useSettingsStore.getState().openSettings();
    usePaneStore.getState().openEphemeralTab();
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('keeps Settings open when a tab opens while locked, and still opens the tab', () => {
    useSettingsStore.getState().openSettings();
    useSettingsStore.getState().setLocked(true);
    usePaneStore.getState().openEphemeralTab();
    expect(useSettingsStore.getState().open).toBe(true);
    const { root } = usePaneStore.getState();
    expect(root.type === 'leaf' && root.tabs.some((t) => t.title === 'Untitled')).toBe(true);
  });

  it('closes Settings when an open collection tab is focused', () => {
    usePaneStore.getState().openTab(collectionTab);
    useSettingsStore.getState().openSettings();
    expect(usePaneStore.getState().openCollectionTab('demo', 'overview')).toBe(true);
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('keeps Settings open when no collection tab is found', () => {
    useSettingsStore.getState().openSettings();
    expect(usePaneStore.getState().openCollectionTab('missing', 'overview')).toBe(false);
    expect(useSettingsStore.getState().open).toBe(true);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx src/stores/__tests__/pane-store-settings.test.ts`
Expected: FAIL. "does not send the hidden request on Ctrl+Enter" with `expected "spy" to not be
called`; "closes Settings instead of the tab on Ctrl+W" with `expected true to be false`; "closes
Settings when a tab opens" with `expected true to be false`.

- [ ] **Step 4: Suppress tab shortcuts in `src/hooks/useKeyboardShortcuts.ts`**

Directly after the Ctrl+, block from plan 01 (still before `const store = usePaneStore.getState();`),
insert:

```ts
      // Settings covers the tabs, so shortcuts that act on the hidden active tab are off.
      const settings = useSettingsStore.getState();
      if (settings.open) {
        if (e.key === 'Enter' || e.key === 's') {
          e.preventDefault();
          return;
        }
        if (e.key === 'w') {
          e.preventDefault();
          settings.closeSettings();
          return;
        }
      }
```

- [ ] **Step 5: Close Settings from `src/stores/pane-store.ts`**

Add the import next to the other store imports:

```ts
import { useSettingsStore } from '@/stores/settings-store';
```

In `openTab(tab, groupId)` (line 467), insert as the first lines of the body, before the
"Exit workspace mode" comment:

```ts
    // The user asked for this tab, so Settings steps aside. A locked Settings stays on top.
    useSettingsStore.getState().closeSettings();
```

In `openCollectionTab(collection, section)` (line 1576), directly after
`if (!target) return false;`, insert:

```ts
    // Focusing a collection tab is a request to see it, so Settings steps aside.
    useSettingsStore.getState().closeSettings();
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `yarn test src/hooks/__tests__ src/stores/__tests__/pane-store-settings.test.ts src/stores/__tests__/pane-store.test.ts`
Expected: PASS (7 settings-shortcut tests, 4 pane-store-settings tests; existing hook and
pane-store tests unchanged).

- [ ] **Step 7: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src/hooks/useKeyboardShortcuts.ts src/hooks/__tests__/useKeyboardShortcuts-settings.test.tsx \
  src/stores/pane-store.ts src/stores/__tests__/pane-store-settings.test.ts
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(settings): keep tab shortcuts off while settings is open`.

---

### Task 2: Proxy section

**Files:**
- Move: `src/components/settings/ProxySettingsDialog.tsx` → `src/components/settings/sections/ProxySection.tsx`
- Move: `src/components/settings/__tests__/ProxySettingsDialog.test.tsx` → `src/components/settings/sections/__tests__/ProxySection.test.tsx`
- Modify: `src/components/settings/settings-sections.ts`
- Modify: `src/components/title-bar/TitleBar.tsx`
- Create: `src/components/title-bar/__tests__/TitleBar.test.tsx`

**Interfaces:**
- Consumes: `getProxySettings`, `saveProxySettings`, `ProxyMode`, `ProxyPasswordChange`
  (`@/lib/tauri-api`); `SettingsSectionHeader` (plan 01).
- Produces: `export function ProxySection(): JSX.Element;` registered as
  `{ id: 'proxy', label: 'Proxy', icon: Globe, Component: ProxySection }`.

- [ ] **Step 1: Move the files**

```bash
git mv src/components/settings/ProxySettingsDialog.tsx src/components/settings/sections/ProxySection.tsx
git mv src/components/settings/__tests__/ProxySettingsDialog.test.tsx src/components/settings/sections/__tests__/ProxySection.test.tsx
```

- [ ] **Step 2: Update the moved test and add the failing tests**

In `src/components/settings/sections/__tests__/ProxySection.test.tsx`:

- Replace `import { ProxySettingsDialog } from '../ProxySettingsDialog';` with
  `import { ProxySection } from '../ProxySection';`.
- Replace every `render(<ProxySettingsDialog open onOpenChange={vi.fn()} />);` with
  `render(<ProxySection />);` (six places).
- Replace `describe('ProxySettingsDialog', () => {` with `describe('ProxySection', () => {`.
- Append inside the `describe` block:

```tsx
  it('has a Proxy heading', async () => {
    render(<ProxySection />);
    expect(screen.getByRole('heading', { name: 'Proxy' })).toBeInTheDocument();
    await screen.findByDisplayValue('http://proxy.corp:8080');
  });

  it('stays on the section and reloads the saved setting after Save', async () => {
    render(<ProxySection />);
    await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.change(screen.getByLabelText('Proxy password'), { target: { value: 'pw' } });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));
    await waitFor(() => expect(getProxySettings).toHaveBeenCalledTimes(2));
    expect(await screen.findByLabelText('Proxy password')).toHaveValue('');
    expect(screen.getByText(/a password is saved/i)).toBeInTheDocument();
  });

  it('drops edits on Reload', async () => {
    render(<ProxySection />);
    const field = await screen.findByDisplayValue('http://proxy.corp:8080');
    fireEvent.change(field, { target: { value: 'http://other:1' } });
    fireEvent.click(screen.getByRole('button', { name: /^reload$/i }));
    expect(await screen.findByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(saveProxySettings).not.toHaveBeenCalled();
  });

  it('retries a failed load on Reload', async () => {
    getProxySettings.mockRejectedValueOnce(new Error('boom'));
    render(<ProxySection />);
    expect(await screen.findByRole('alert')).toHaveTextContent(/use reload to try again/i);
    fireEvent.click(screen.getByRole('button', { name: /^reload$/i }));
    expect(await screen.findByDisplayValue('http://proxy.corp:8080')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^save$/i })).toBeEnabled();
  });
```

`vi` stays imported: the module mocks still use it.

- [ ] **Step 3: Run the test to verify it fails**

Run: `yarn test src/components/settings/sections/__tests__/ProxySection.test.tsx`
Expected: FAIL with `does not provide an export named 'ProxySection'`.

- [ ] **Step 4: Rewrite `src/components/settings/sections/ProxySection.tsx`**

Replace the whole file with:

```tsx
import { Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import {
  getProxySettings,
  type ProxyMode,
  type ProxyPasswordChange,
  saveProxySettings,
} from '@/lib/tauri-api';

const MODES: { value: ProxyMode; label: string; hint: string }[] = [
  {
    value: 'system',
    label: 'System',
    hint: 'Use the HTTP_PROXY, HTTPS_PROXY and NO_PROXY variables.',
  },
  { value: 'none', label: 'None', hint: 'Always connect directly.' },
  {
    value: 'custom',
    label: 'Custom',
    hint: 'Use the proxy URLs below. A single URL is used for both HTTP and HTTPS.',
  },
];

export function ProxySection() {
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [saving, setSaving] = useState(false);
  // Bumped to load the stored setting again, after a save or on Reload.
  const [loadCount, setLoadCount] = useState(0);
  const [mode, setMode] = useState<ProxyMode>('system');
  const [httpProxy, setHttpProxy] = useState('');
  const [httpsProxy, setHttpsProxy] = useState('');
  const [noProxy, setNoProxy] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [hasPassword, setHasPassword] = useState(false);
  const [removePassword, setRemovePassword] = useState(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: loadCount is the reload trigger.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setLoadFailed(false);
    getProxySettings()
      .then((s) => {
        if (cancelled) return;
        setMode(s.mode);
        setHttpProxy(s.httpProxy ?? '');
        setHttpsProxy(s.httpsProxy ?? '');
        setNoProxy(s.noProxy ?? '');
        setUsername(s.username ?? '');
        setHasPassword(s.hasPassword);
        setPassword('');
        setRemovePassword(false);
      })
      .catch(() => {
        if (cancelled) return;
        // Saving now would overwrite the stored setting with the defaults shown.
        setLoadFailed(true);
        toast.error('Could not load the proxy settings');
      })
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [loadCount]);

  const reload = () => setLoadCount((c) => c + 1);

  const handleSave = async () => {
    const change: ProxyPasswordChange = password
      ? { action: 'set', value: password }
      : removePassword
        ? { action: 'clear' }
        : { action: 'keep' };
    setSaving(true);
    try {
      await saveProxySettings({ mode, httpProxy, httpsProxy, noProxy, username }, change);
      toast.success('Proxy settings saved');
      reload();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <SettingsSectionHeader
        title='Proxy'
        description='How Rocket reaches the network for requests and OAuth 2.0 token calls.'
      />
      {loading ? (
        <div className='flex justify-center p-6'>
          <Loader2 className='h-4 w-4 animate-spin' aria-label='Loading' />
        </div>
      ) : loadFailed ? (
        <p role='alert' className='text-sm text-destructive'>
          Could not load the proxy settings. Use Reload to try again.
        </p>
      ) : (
        <div className='space-y-4'>
          <RadioGroup value={mode} onValueChange={(v) => setMode(v as ProxyMode)}>
            {MODES.map((m) => (
              <div key={m.value} className='flex items-start gap-2'>
                <RadioGroupItem value={m.value} id={`proxy-${m.value}`} className='mt-0.5' />
                <Label htmlFor={`proxy-${m.value}`} className='flex flex-col gap-0.5'>
                  <span>{m.label}</span>
                  <span className='text-xs font-normal text-muted-foreground'>{m.hint}</span>
                </Label>
              </div>
            ))}
          </RadioGroup>
          {mode === 'custom' && (
            <div className='space-y-3'>
              <div className='space-y-1.5'>
                <Label htmlFor='proxy-http'>HTTP proxy URL</Label>
                <Input
                  id='proxy-http'
                  aria-label='HTTP proxy URL'
                  placeholder='http://proxy.corp:8080'
                  value={httpProxy}
                  onChange={(e) => setHttpProxy(e.target.value)}
                />
              </div>
              <div className='space-y-1.5'>
                <Label htmlFor='proxy-https'>HTTPS proxy URL</Label>
                <Input
                  id='proxy-https'
                  aria-label='HTTPS proxy URL'
                  placeholder='http://proxy.corp:8080'
                  value={httpsProxy}
                  onChange={(e) => setHttpsProxy(e.target.value)}
                />
              </div>
              <div className='space-y-1.5'>
                <Label htmlFor='proxy-no'>No proxy for</Label>
                <Input
                  id='proxy-no'
                  aria-label='No proxy for'
                  placeholder='localhost, .internal.corp'
                  value={noProxy}
                  onChange={(e) => setNoProxy(e.target.value)}
                />
              </div>
              <div className='grid grid-cols-2 gap-2'>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-user'>Username</Label>
                  <Input
                    id='proxy-user'
                    aria-label='Proxy username'
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                  />
                </div>
                <div className='space-y-1.5'>
                  <Label htmlFor='proxy-pass'>Password</Label>
                  <Input
                    id='proxy-pass'
                    aria-label='Proxy password'
                    type='password'
                    autoComplete='new-password'
                    placeholder={hasPassword && !removePassword ? 'Unchanged' : ''}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </div>
              </div>
              {hasPassword && !removePassword && (
                <p className='flex items-center gap-2 text-xs text-muted-foreground'>
                  A password is saved in the system keychain.
                  <Button
                    variant='link'
                    size='sm'
                    className='h-auto p-0 text-xs'
                    onClick={() => {
                      // Removing means no password at all, so a typed one is dropped too.
                      setPassword('');
                      setRemovePassword(true);
                    }}
                  >
                    Remove saved password
                  </Button>
                </p>
              )}
              <p className='text-xs text-muted-foreground'>
                OAuth 2.0 token requests (token, refresh and authorization-code exchange) also use
                this setting.
              </p>
            </div>
          )}
        </div>
      )}
      <div className='mt-6 flex justify-end gap-2'>
        <Button variant='ghost' onClick={reload} disabled={loading || saving}>
          Reload
        </Button>
        <Button onClick={handleSave} disabled={loading || saving || loadFailed}>
          Save
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Register the section in `src/components/settings/settings-sections.ts`**

Change the lucide import to `import { Globe, Info, LifeBuoy, type LucideIcon } from 'lucide-react';`,
add `import { ProxySection } from '@/components/settings/sections/ProxySection';`, and make the list:

```ts
export const SETTINGS_SECTIONS: SettingsSectionDef[] = [
  { id: 'proxy', label: 'Proxy', icon: Globe, Component: ProxySection },
  { id: 'support', label: 'Support', icon: LifeBuoy, Component: SupportSection },
  { id: 'about', label: 'About', icon: Info, Component: AboutSection },
];
```

In `src/components/settings/__tests__/SettingsPage.test.tsx` the first section is now Proxy, which
calls `getProxySettings`. Update that file:

- Change the `@/lib/tauri-api` mock to
  `vi.mock('@/lib/tauri-api', () => ({ getAppDataDir: vi.fn().mockResolvedValue('/data'), getProxySettings: vi.fn().mockResolvedValue({ mode: 'system', hasPassword: false }), saveProxySettings: vi.fn() }));`
- In "lists the sections in order…", expect `['Proxy', 'Support', 'About']`, the heading `'Proxy'`
  and `aria-current='page'` on `'Proxy'`.
- Rename "shows the first section for a section that is not registered" to use `setSection('ai')`
  and expect the heading `'Proxy'`.
- In "disables Close and the other sections while locked", expect `'About'` disabled and `'Proxy'`
  enabled (the active one) instead of `'Support'`.

- [ ] **Step 6: Remove the globe button from `src/components/title-bar/TitleBar.tsx`**

Replace the whole file with:

```tsx
import { type as osType } from '@tauri-apps/plugin-os';
import { Bot, Settings } from 'lucide-react';
import { useState } from 'react';
import { AssistantToggleButton } from '@/components/assistant/AssistantToggleButton';
import { AgentConfigsDialog } from '@/components/settings/AgentConfigsDialog';
import { SecretManagerConnectionsDialog } from '@/components/settings/SecretManagerConnectionsDialog';
import { Button } from '@/components/ui/button';
import { WindowControls } from './WindowControls';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export function TitleBar() {
  const isMac = osType() === 'macos';
  const [showSecretManagers, setShowSecretManagers] = useState(false);
  const [showAgentConfigs, setShowAgentConfigs] = useState(false);

  return (
    <div
      className='relative grid h-10 w-full grid-cols-[1fr_auto_1fr] items-center shrink-0 border-b border-titlebar-border bg-titlebar-bg text-titlebar-fg'
      data-tauri-drag-region
    >
      <div className='flex items-center h-full' data-tauri-drag-region>
        {isMac && <div className='w-[72px] shrink-0' data-tauri-drag-region />}

        <div className='flex items-center gap-2 px-3 shrink-0'>
          <img src='/rocket.png' alt='Rocket' className='h-4 w-4' />
          <span className='text-sm font-medium'>Rocket</span>
        </div>
      </div>

      <WorkspaceSwitcher />

      <div className='flex items-center justify-end h-full gap-1' data-tauri-drag-region>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Secret Manager connections'
          onClick={() => setShowSecretManagers(true)}
        >
          <Settings className='h-4 w-4' aria-hidden='true' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='AI Agent configurations'
          onClick={() => setShowAgentConfigs(true)}
        >
          <Bot className='h-4 w-4' aria-hidden='true' />
        </Button>
        <AssistantToggleButton />
        {!isMac && <WindowControls />}
      </div>

      <SecretManagerConnectionsDialog
        open={showSecretManagers}
        onOpenChange={setShowSecretManagers}
      />
      <AgentConfigsDialog open={showAgentConfigs} onOpenChange={setShowAgentConfigs} />
    </div>
  );
}
```

- [ ] **Step 7: Create `src/components/title-bar/__tests__/TitleBar.test.tsx`**

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TitleBar } from '../TitleBar';

vi.mock('@tauri-apps/plugin-os', () => ({ type: () => 'linux' }));
vi.mock('../WorkspaceSwitcher', () => ({ WorkspaceSwitcher: () => null }));
vi.mock('../WindowControls', () => ({ WindowControls: () => null }));
vi.mock('@/components/assistant/AssistantToggleButton', () => ({
  AssistantToggleButton: () => null,
}));
// Removed in the next tasks, together with these mocks.
vi.mock('@/components/settings/SecretManagerConnectionsDialog', () => ({
  SecretManagerConnectionsDialog: () => null,
}));
vi.mock('@/components/settings/AgentConfigsDialog', () => ({ AgentConfigsDialog: () => null }));

describe('TitleBar', () => {
  it('has no Proxy button, since Proxy lives in Settings', () => {
    render(<TitleBar />);
    expect(screen.queryByRole('button', { name: 'Proxy settings' })).toBeNull();
  });
});
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `yarn test src/components/settings src/components/title-bar`
Expected: PASS (ProxySection: 6 moved tests + 4 new; SettingsPage updated; TitleBar 1).

- [ ] **Step 9: Check types, lint and leftovers**

Run: `yarn tsc --noEmit && yarn check && grep -rn "ProxySettingsDialog" src`
Expected: no type or lint errors; `grep` prints nothing.

- [ ] **Step 10: Commit**

```bash
git add src/components/settings/ProxySettingsDialog.tsx \
  src/components/settings/sections/ProxySection.tsx \
  src/components/settings/__tests__/ProxySettingsDialog.test.tsx \
  src/components/settings/sections/__tests__/ProxySection.test.tsx \
  src/components/settings/settings-sections.ts \
  src/components/settings/__tests__/SettingsPage.test.tsx \
  src/components/title-bar/TitleBar.tsx \
  src/components/title-bar/__tests__/TitleBar.test.tsx
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(settings): move the proxy dialog into settings`.

---

### Task 3: Secret Managers section and the title-bar gear

**Files:**
- Move: `src/components/settings/SecretManagerConnectionsDialog.tsx` → `src/components/settings/sections/SecretManagersSection.tsx`
- Move: `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx` → `src/components/settings/sections/__tests__/SecretManagersSection.test.tsx`
- Modify: `src/components/settings/settings-sections.ts`
- Modify: `src/components/settings/__tests__/SettingsPage.test.tsx` (section list)
- Modify: `src/components/title-bar/TitleBar.tsx`
- Modify: `src/components/title-bar/__tests__/TitleBar.test.tsx`

**Interfaces:**
- Consumes: the Secret Manager query hooks (`@/lib/queries/secret-manager-queries`), the
  provider helpers (`@/lib/secret-providers`), `SettingsSectionHeader`, `SettingsPage`,
  `useSettingsStore` (plan 01).
- Produces: `export function SecretManagersSection(): JSX.Element;` registered as
  `{ id: 'secret-managers', label: 'Secret Managers', icon: KeyRound, Component: SecretManagersSection, wide: true }`.

- [ ] **Step 1: Move the files**

```bash
git mv src/components/settings/SecretManagerConnectionsDialog.tsx src/components/settings/sections/SecretManagersSection.tsx
git mv src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx src/components/settings/sections/__tests__/SecretManagersSection.test.tsx
```

- [ ] **Step 2: Update the moved test and add the failing tests**

In `src/components/settings/sections/__tests__/SecretManagersSection.test.tsx`:

- Replace the `SecretManagerConnectionsDialog` import with:

```tsx
import { SettingsPage } from '@/components/settings/SettingsPage';
import { useSettingsStore } from '@/stores/settings-store';
import { SecretManagersSection } from '../SecretManagersSection';
```

- Replace the body of `renderDialog()` (rename it `renderSection()` and update its callers with
  find-and-replace) with:

```tsx
function renderSection(ui: ReactNode = <SecretManagersSection />) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}
```

  and add `import type { ReactNode } from 'react';`.
- Replace `describe('SecretManagerConnectionsDialog', () => {` with
  `describe('SecretManagersSection', () => {`.
- Below the imports, add the jsdom polyfills Radix Select needs:

```tsx
// Radix Select calls pointer-capture and scroll APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}
```

- Append inside the `describe` block:

```tsx
  it('has a heading', async () => {
    renderSection();
    expect(screen.getByRole('heading', { name: 'Secret Manager Connections' })).toBeInTheDocument();
  });

  it('drops an open edit form when the section unmounts', async () => {
    const client = new QueryClient();
    const user = userEvent.setup();
    const view = render(
      <QueryClientProvider client={client}>
        <SecretManagersSection />
      </QueryClientProvider>,
    );
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    expect(screen.getByLabelText('Label')).toBeInTheDocument();
    view.unmount();
    render(
      <QueryClientProvider client={client}>
        <SecretManagersSection />
      </QueryClientProvider>,
    );
    expect(await screen.findByRole('button', { name: /add connection/i })).toBeInTheDocument();
    expect(screen.queryByLabelText('Label')).toBeNull();
  });

  it('keeps Settings open when Esc closes the provider list', async () => {
    useSettingsStore.getState().reset();
    useSettingsStore.getState().openSettings('secret-managers');
    renderSection(<SettingsPage />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.click(screen.getByRole('combobox', { name: /provider/i }));
    expect(await screen.findByRole('listbox')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('listbox')).toBeNull();
    expect(useSettingsStore.getState().open).toBe(true);
  });
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `yarn test src/components/settings/sections/__tests__/SecretManagersSection.test.tsx`
Expected: FAIL with `does not provide an export named 'SecretManagersSection'`.

- [ ] **Step 4: Turn `src/components/settings/sections/SecretManagersSection.tsx` into a section**

Five edits, nothing else in the file changes:

1. Line 2: `import { useEffect, useState } from 'react';` → `import { useState } from 'react';`.
2. Line 5: replace `import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';`
   with `import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';`.
3. Delete the props interface (lines 31-34):

```tsx
interface SecretManagerConnectionsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}
```

4. Replace the signature (lines 49-52)

```tsx
export function SecretManagerConnectionsDialog({
  open,
  onOpenChange,
}: SecretManagerConnectionsDialogProps) {
```

   with `export function SecretManagersSection() {`, and delete the reset effect (lines 72-78):

```tsx
  useEffect(() => {
    if (!open) {
      setEditing(null);
      setTestVaultNames({});
      setDeletingId(null);
    }
  }, [open]);
```

   The section unmounts when the user leaves it or closes Settings, which resets the same state.
5. Replace the wrapper opening (lines 178-183)

```tsx
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='w-auto min-w-[28rem] max-w-[min(90vw,_48rem)]'>
        <DialogHeader>
          <DialogTitle>Secret Manager Connections</DialogTitle>
        </DialogHeader>
```

   with

```tsx
  return (
    <div>
      <SettingsSectionHeader
        title='Secret Manager Connections'
        description='Connections to external secret stores, used by environments and AI Agents.'
      />
```

   and the wrapper closing at the end of the file

```tsx
      </DialogContent>
    </Dialog>
  );
}
```

   with

```tsx
    </div>
  );
}
```

Then run `yarn lint` so Biome re-indents the body.

- [ ] **Step 5: Register the section**

In `src/components/settings/settings-sections.ts`, add `KeyRound` to the lucide import, add
`import { SecretManagersSection } from '@/components/settings/sections/SecretManagersSection';`,
and insert after the Proxy row:

```ts
  {
    id: 'secret-managers',
    label: 'Secret Managers',
    icon: KeyRound,
    Component: SecretManagersSection,
    wide: true,
  },
```

In `src/components/settings/__tests__/SettingsPage.test.tsx`, expect
`['Proxy', 'Secret Managers', 'Support', 'About']` in "lists the sections in order…".

- [ ] **Step 6: Point the gear at Settings in `src/components/title-bar/TitleBar.tsx`**

Replace the whole file with:

```tsx
import { type as osType } from '@tauri-apps/plugin-os';
import { Bot, Settings } from 'lucide-react';
import { useState } from 'react';
import { AssistantToggleButton } from '@/components/assistant/AssistantToggleButton';
import { AgentConfigsDialog } from '@/components/settings/AgentConfigsDialog';
import { Button } from '@/components/ui/button';
import { useSettingsStore } from '@/stores/settings-store';
import { WindowControls } from './WindowControls';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export function TitleBar() {
  const isMac = osType() === 'macos';
  const toggleSettings = useSettingsStore((s) => s.toggleSettings);
  const [showAgentConfigs, setShowAgentConfigs] = useState(false);

  return (
    <div
      className='relative grid h-10 w-full grid-cols-[1fr_auto_1fr] items-center shrink-0 border-b border-titlebar-border bg-titlebar-bg text-titlebar-fg'
      data-tauri-drag-region
    >
      <div className='flex items-center h-full' data-tauri-drag-region>
        {isMac && <div className='w-[72px] shrink-0' data-tauri-drag-region />}

        <div className='flex items-center gap-2 px-3 shrink-0'>
          <img src='/rocket.png' alt='Rocket' className='h-4 w-4' />
          <span className='text-sm font-medium'>Rocket</span>
        </div>
      </div>

      <WorkspaceSwitcher />

      <div className='flex items-center justify-end h-full gap-1' data-tauri-drag-region>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Settings'
          onClick={toggleSettings}
        >
          <Settings className='h-4 w-4' aria-hidden='true' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='AI Agent configurations'
          onClick={() => setShowAgentConfigs(true)}
        >
          <Bot className='h-4 w-4' aria-hidden='true' />
        </Button>
        <AssistantToggleButton />
        {!isMac && <WindowControls />}
      </div>

      <AgentConfigsDialog open={showAgentConfigs} onOpenChange={setShowAgentConfigs} />
    </div>
  );
}
```

- [ ] **Step 7: Extend `src/components/title-bar/__tests__/TitleBar.test.tsx`**

Delete the `SecretManagerConnectionsDialog` mock (the module no longer exists). Add
`import userEvent from '@testing-library/user-event';`,
`import { beforeEach } from 'vitest';` (merge into the existing vitest import) and
`import { useSettingsStore } from '@/stores/settings-store';`, then add inside the `describe`:

```tsx
  beforeEach(() => {
    useSettingsStore.getState().reset();
  });

  it('opens and closes Settings with the gear', async () => {
    render(<TitleBar />);
    const gear = screen.getByRole('button', { name: 'Settings' });
    await userEvent.click(gear);
    expect(useSettingsStore.getState().open).toBe(true);
    await userEvent.click(gear);
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('has no Secret Manager button, since Secret Managers live in Settings', () => {
    render(<TitleBar />);
    expect(screen.queryByRole('button', { name: 'Secret Manager connections' })).toBeNull();
  });
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `yarn test src/components/settings src/components/title-bar src/components/environments`
Expected: PASS (SecretManagersSection: moved tests + 3 new; TitleBar 3; the environments tests,
which use the same Secret Manager queries, unchanged).

- [ ] **Step 9: Check types, lint and leftovers**

Run: `yarn tsc --noEmit && yarn check && grep -rn "SecretManagerConnectionsDialog" src`
Expected: no type or lint errors; `grep` prints nothing.

- [ ] **Step 10: Commit**

```bash
git add src/components/settings/SecretManagerConnectionsDialog.tsx \
  src/components/settings/sections/SecretManagersSection.tsx \
  src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx \
  src/components/settings/sections/__tests__/SecretManagersSection.test.tsx \
  src/components/settings/settings-sections.ts \
  src/components/settings/__tests__/SettingsPage.test.tsx \
  src/components/title-bar/TitleBar.tsx \
  src/components/title-bar/__tests__/TitleBar.test.tsx
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(settings): move secret managers into settings`.

---

## End of Plan 2. Verify

- `yarn tsc --noEmit` and `yarn check` pass.
- `yarn test src/components/settings src/components/title-bar src/components/environments src/hooks/__tests__ src/stores/__tests__/pane-store-settings.test.ts src/stores/__tests__/pane-store.test.ts` passes.
- Manual (`yarn tauri dev`):
  1. The gear opens Settings at Proxy; edit, Save, see the toast and the reloaded values.
  2. Secret Managers: add, test, edit and delete a connection; an environment's External Secrets
     tab sees the change.
  3. With Settings open, Ctrl+W closes Settings and the tab underneath is still there; Ctrl+S does
     nothing; clicking a request in the sidebar closes Settings and shows the request.

Next: `2026-10-31-settings-shell-plan-03-ai-agents-and-cleanup.md`.

## Self-Review

- Spec coverage: 4.3 `openTab` row (Task 1), 4.4 (Task 1), 4.6 Proxy row (Task 2), 4.6 Secret
  Managers row (Task 3), 4.8 title bar gear and globe (Tasks 2 and 3), 6 moved Proxy and Secret
  Managers tests (Tasks 2 and 3).
- Names: `ProxySection`, `SecretManagersSection`; `SettingsSection` ids `'proxy'` and
  `'secret-managers'` match plan 01.
- Not covered here: AI Agents and its save lock, the assistant hint, docs (plan 03).
