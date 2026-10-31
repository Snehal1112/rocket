# Settings Shell — Plan 3: AI Agents and cleanup

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 3 of 3.** Previous plan: `docs/superpowers/plans/settings-page-shell/2026-10-31-settings-shell-plan-02-shortcuts-proxy-secrets.md` (must be merged and green).
**Next plan to run: none. After this plan, run the final whole-branch review.**
**Recommended model: Sonnet.**

**Goal:** AI Agents moves into Settings and locks the page while a save is in flight, so the
credential write is never lost. The assistant's "no agents" hint opens it. The feature is
documented and the roadmap updated.

**Architecture:** `AgentConfigsDialog` is moved with `git mv` to
`src/components/settings/sections/AiAgentsSection.tsx` and loses its `<Dialog>` wrapper. The old
"no close mid-save" rule (`onOpenChange` ignored while `saving`) becomes `setLocked(saving)` on the
settings store, plus `setLocked(false)` on unmount. Every way out of Settings (Close, Esc, Ctrl+W,
Ctrl+,, the gear, opening a tab, switching sections) already checks the lock (plans 01 and 02).
`AssistantStartView` gets a button that calls `openSettings('ai')`.

**Tech Stack:** React 19, TypeScript, Zustand 5, TanStack Query, shadcn/ui, lucide-react, Vitest +
Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-31-settings-page-shell-design.md`, sections 4.3
(assistant row), 4.6 (AI Agents row), 4.8, 6. Prerequisites from plans 01 and 02:
`useSettingsStore` with `setLocked`, `SETTINGS_SECTIONS`, `SettingsSectionHeader`, the title bar
with the gear, the lock checks in `useKeyboardShortcuts` and `pane-store`.

## Global Constraints

- shadcn/ui primitives only; lucide-react icons only; narrow Zustand selectors.
- AI Agents keeps its state, queries, mutations, validation, credential handling and messages. Only
  the dialog wrapper, the `open`/`onOpenChange` props and the reset-on-close effect go.
- The lock is held from Save until the agent and its credential are both written, success or
  failure, and is released on unmount.
- The assistant composer's per-agent model and effort picks stay in the assistant panel.
- Docs go in `docs/features/`, with a one-line pointer in the project `CLAUDE.md`.
- Comments are short full sentences ending with a period.
- Verification per task: `yarn tsc --noEmit`, `yarn check`, the task's `yarn test <path>`.

## Review Focus

- **A save that fails.** The lock must be released, or the user is stuck in Settings. Covered:
  Task 1, "unlocks Settings when the save fails".
- **The keychain step fails after the agent is saved.** The form stays open and the lock is
  released. Covered: Task 1, the moved test "keeps the form open and hides the value when the
  credential cannot be saved" plus an added lock assertion.
- **The section unmounts mid-save** (app teardown, an error boundary). The lock must not outlive
  the section. Covered: Task 1, "unlocks Settings when the section unmounts mid-save".
- **No agents configured on first launch.** The assistant hint must lead somewhere now that the
  bot icon is gone. Covered: Task 2, "opens Settings at AI Agents from the no-agents hint".
- **Old title-bar entry points.** No button may still open a removed dialog. Covered: Task 1,
  "has only the Settings gear for settings", and the `grep` leftovers check.

## File Structure

| File | Change |
|---|---|
| `src/components/settings/AgentConfigsDialog.tsx` → `src/components/settings/sections/AiAgentsSection.tsx` | **Move**, drop the dialog wrapper, add the lock |
| `src/components/settings/__tests__/AgentConfigsDialog.test.tsx` → `src/components/settings/sections/__tests__/AiAgentsSection.test.tsx` | **Move**, replace dialog tests with lock tests |
| `src/components/settings/settings-sections.ts` | Register AI Agents |
| `src/components/settings/__tests__/SettingsPage.test.tsx` | Section list |
| `src/components/title-bar/TitleBar.tsx` | Drop the bot button |
| `src/components/title-bar/__tests__/TitleBar.test.tsx` | Drop the dialog mock, add a test |
| `src/components/assistant/AssistantStartView.tsx` | "Open AI Agents settings" button (line 108-112) |
| `src/components/assistant/__tests__/AssistantStartView-no-agents.test.tsx` | **Create** |
| `docs/features/settings-page.md` | **Create**: feature doc |
| `CLAUDE.md` | Pointer line |
| `docs/roadmaps/roadmap.md` | R-72 status |

---

### Task 1: AI Agents section with the save lock

**Files:**
- Move: `src/components/settings/AgentConfigsDialog.tsx` → `src/components/settings/sections/AiAgentsSection.tsx`
- Move: `src/components/settings/__tests__/AgentConfigsDialog.test.tsx` → `src/components/settings/sections/__tests__/AiAgentsSection.test.tsx`
- Modify: `src/components/settings/settings-sections.ts`
- Modify: `src/components/settings/__tests__/SettingsPage.test.tsx`
- Modify: `src/components/title-bar/TitleBar.tsx`
- Modify: `src/components/title-bar/__tests__/TitleBar.test.tsx`

**Interfaces:**
- Consumes: `useSettingsStore` `setLocked` (plan 01); `SettingsSectionHeader` (plan 01); the
  agent-config form modules under `src/components/settings/agent-config/` (unchanged).
- Produces: `export function AiAgentsSection(): JSX.Element;` registered as
  `{ id: 'ai', label: 'AI Agents', icon: Bot, Component: AiAgentsSection, wide: true }`.

- [ ] **Step 1: Move the files**

```bash
git mv src/components/settings/AgentConfigsDialog.tsx src/components/settings/sections/AiAgentsSection.tsx
git mv src/components/settings/__tests__/AgentConfigsDialog.test.tsx src/components/settings/sections/__tests__/AiAgentsSection.test.tsx
```

- [ ] **Step 2: Update the moved test and write the failing lock tests**

In `src/components/settings/sections/__tests__/AiAgentsSection.test.tsx`:

1. Replace `import { AgentConfigsDialog } from '@/components/settings/AgentConfigsDialog';` with:

```tsx
import { useSettingsStore } from '@/stores/settings-store';
import { AiAgentsSection } from '../AiAgentsSection';
```

2. Replace `renderDialog` (lines 57-66) with the version below, and rename every call
   `renderDialog(` to `renderSection(`:

```tsx
function renderSection() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AiAgentsSection />
    </QueryClientProvider>,
  );
}
```

3. `describe('AgentConfigsDialog', () => {` → `describe('AiAgentsSection', () => {`, and add as
   the first line of its `beforeEach`: `useSettingsStore.getState().reset();`.
4. Replace the test "has a title and a description" (lines 125-129) with:

```tsx
    it('has a title and a description', async () => {
      renderSection();
      expect(await screen.findByRole('heading', { name: 'AI Agents' })).toBeInTheDocument();
      expect(
        screen.getByText('The agents Rocket can start for the workspace assistant.'),
      ).toBeInTheDocument();
    });
```

5. Delete the test "lays the dialog out as a column so the body scrolls and the footer is pinned"
   (lines 131-135). It asserted dialog-only classes.
6. Replace the test "does not let the dialog close mid-save" (lines 333-348) with:

```tsx
    it('locks Settings while saving and unlocks when the save ends', async () => {
      const release = deferSave();
      const user = await openEdit();
      await clickSave(user);
      expect(useSettingsStore.getState().locked).toBe(true);
      release();
      await screen.findByText('claude-agent-acp');
      expect(useSettingsStore.getState().locked).toBe(false);
    });

    it('unlocks Settings when the save fails', async () => {
      vi.mocked(api.saveAgentConfig).mockRejectedValue(new Error('nope'));
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
      expect(useSettingsStore.getState().locked).toBe(false);
    });

    it('unlocks Settings when the section unmounts mid-save', async () => {
      deferSave();
      const user = userEvent.setup();
      const { unmount } = renderSection();
      await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
      await screen.findByLabelText('Label');
      await clickSave(user);
      expect(useSettingsStore.getState().locked).toBe(true);
      unmount();
      expect(useSettingsStore.getState().locked).toBe(false);
    });
```

7. Rename the test "keeps the dialog open and hides the value when the credential cannot be saved"
   to "keeps the form open and hides the value when the credential cannot be saved", and add as its
   last line: `expect(useSettingsStore.getState().locked).toBe(false);`.

- [ ] **Step 3: Run the test to verify it fails**

Run: `yarn test src/components/settings/sections/__tests__/AiAgentsSection.test.tsx`
Expected: FAIL with `does not provide an export named 'AiAgentsSection'`.

- [ ] **Step 4: Turn `src/components/settings/sections/AiAgentsSection.tsx` into a section**

1. Replace the dialog import block (lines 18-26)

```tsx
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
```

   with `import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';`, and
   add `import { useSettingsStore } from '@/stores/settings-store';` after the
   `@/lib/tauri-api` import.
2. Delete the props interface (lines 42-45):

```tsx
interface AgentConfigsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}
```

3. Replace `export function AgentConfigsDialog({ open, onOpenChange }: AgentConfigsDialogProps) {`
   (line 94) with `export function AiAgentsSection() {`.
4. Replace the reset effect (lines 114-121)

```tsx
  useEffect(() => {
    if (!open) {
      setEditing(null);
      setDeletingId(null);
      credentialCheckId.current = null;
      setHasSavedCredential(false);
    }
  }, [open]);
```

   with

```tsx
  const setLocked = useSettingsStore((s) => s.setLocked);
  // Leaving Settings mid-save would drop the form before the credential is written.
  useEffect(() => {
    setLocked(saving);
  }, [saving, setLocked]);
  // The lock must not outlive the section.
  useEffect(() => () => setLocked(false), [setLocked]);
```

   The form state resets on unmount, which replaces the old reset on close.
5. Replace the whole `return ( <Dialog …> … </Dialog> );` block at the end of the component
   (lines 301-355) with:

```tsx
  return (
    <div>
      <SettingsSectionHeader
        title='AI Agents'
        description={
          editing
            ? 'Set how Rocket starts this agent and where its credential comes from.'
            : 'The agents Rocket can start for the workspace assistant.'
        }
      />
      {editing ? (
        <AgentConfigForm
          key={formKey}
          value={editing}
          onChange={setEditing}
          hasSavedCredential={hasSavedCredential}
        />
      ) : (
        renderList()
      )}
      {editing && (
        <div className='mt-6 flex items-center justify-end gap-2 border-t pt-4'>
          {saveHelp && (
            <p id={SAVE_HELP_ID} className='mr-auto text-xs text-muted-foreground'>
              {saveHelp}
            </p>
          )}
          <Button variant='outline' size='sm' onClick={cancelEdit} disabled={saving}>
            Cancel
          </Button>
          <Button
            size='sm'
            onClick={() => void handleSave()}
            disabled={saving || saveMutation.isPending || !canSave}
            aria-busy={saving}
            aria-describedby={saveHelp ? SAVE_HELP_ID : undefined}
          >
            {saveMutation.isPending && <Loader2 className='h-3.5 w-3.5 animate-spin' />}
            Save
          </Button>
        </div>
      )}
    </div>
  );
```

- [ ] **Step 5: Register the section**

In `src/components/settings/settings-sections.ts`, add `Bot` to the lucide import, add
`import { AiAgentsSection } from '@/components/settings/sections/AiAgentsSection';`, and insert after
the Secret Managers row:

```ts
  { id: 'ai', label: 'AI Agents', icon: Bot, Component: AiAgentsSection, wide: true },
```

In `src/components/settings/__tests__/SettingsPage.test.tsx`:

- Expect `['Proxy', 'Secret Managers', 'AI Agents', 'Support', 'About']` in "lists the sections in
  order…".
- "shows the first section for a section that is not registered" has no unregistered id left.
  Replace it with:

```tsx
  it('opens at the section it was asked for', () => {
    useSettingsStore.getState().openSettings('about');
    render(<SettingsPage />);
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
  });
```

- [ ] **Step 6: Remove the bot button from `src/components/title-bar/TitleBar.tsx`**

Replace the whole file with:

```tsx
import { type as osType } from '@tauri-apps/plugin-os';
import { Settings } from 'lucide-react';
import { AssistantToggleButton } from '@/components/assistant/AssistantToggleButton';
import { Button } from '@/components/ui/button';
import { useSettingsStore } from '@/stores/settings-store';
import { WindowControls } from './WindowControls';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export function TitleBar() {
  const isMac = osType() === 'macos';
  const toggleSettings = useSettingsStore((s) => s.toggleSettings);

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
        <AssistantToggleButton />
        {!isMac && <WindowControls />}
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Update `src/components/title-bar/__tests__/TitleBar.test.tsx`**

Delete the `AgentConfigsDialog` mock and its "Removed in the next tasks" comment. Replace the
"has no Proxy button…" and "has no Secret Manager button…" tests with:

```tsx
  it('has only the Settings gear for settings', () => {
    render(<TitleBar />);
    expect(screen.getByRole('button', { name: 'Settings' })).toBeInTheDocument();
    for (const name of [
      'Proxy settings',
      'Secret Manager connections',
      'AI Agent configurations',
    ]) {
      expect(screen.queryByRole('button', { name })).toBeNull();
    }
  });
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `yarn test src/components/settings src/components/title-bar src/components/assistant`
Expected: PASS (AiAgentsSection: moved tests minus the deleted one, plus 3 lock tests; the
agent-config form tests unchanged; SettingsPage; TitleBar 2).

- [ ] **Step 9: Check types, lint and leftovers**

Run: `yarn tsc --noEmit && yarn check && grep -rn "AgentConfigsDialog\|ProxySettingsDialog\|SecretManagerConnectionsDialog" src`
Expected: no type or lint errors; `grep` prints nothing.

- [ ] **Step 10: Commit**

```bash
git add src/components/settings/AgentConfigsDialog.tsx \
  src/components/settings/sections/AiAgentsSection.tsx \
  src/components/settings/__tests__/AgentConfigsDialog.test.tsx \
  src/components/settings/sections/__tests__/AiAgentsSection.test.tsx \
  src/components/settings/settings-sections.ts \
  src/components/settings/__tests__/SettingsPage.test.tsx \
  src/components/title-bar/TitleBar.tsx \
  src/components/title-bar/__tests__/TitleBar.test.tsx
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(settings): move AI agents into settings with a save lock`.

---

### Task 2: Assistant hint, docs and roadmap

**Files:**
- Modify: `src/components/assistant/AssistantStartView.tsx` (lucide import lines 1-9; lines 108-112)
- Create: `src/components/assistant/__tests__/AssistantStartView-no-agents.test.tsx`
- Create: `docs/features/settings-page.md`
- Modify: `CLAUDE.md` (after the `ipc-command-acl.md` line)
- Modify: `docs/roadmaps/roadmap.md` (row R-72)

**Interfaces:**
- Consumes: `useSettingsStore` `openSettings` (plan 01).
- Produces: no new names.

- [ ] **Step 1: Write the failing test**

Create `src/components/assistant/__tests__/AssistantStartView-no-agents.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';
import { useSettingsStore } from '@/stores/settings-store';
import { AssistantStartView } from '../AssistantStartView';

vi.mock('@/lib/assistant/assistant-session', () => ({ startAssistant: vi.fn() }));
vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [] }),
}));

describe('AssistantStartView with no agents', () => {
  beforeEach(() => {
    useAssistantStore.getState().reset();
    useSettingsStore.getState().reset();
  });

  it('opens Settings at AI Agents from the no-agents hint', async () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <AssistantStartView />
      </QueryClientProvider>,
    );
    expect(screen.getByText('No agents are configured yet.')).toBeInTheDocument();
    expect(screen.queryByText(/title bar/i)).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Open AI Agents settings' }));
    expect(useSettingsStore.getState()).toMatchObject({ open: true, section: 'ai' });
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/assistant/__tests__/AssistantStartView-no-agents.test.tsx`
Expected: FAIL with `Unable to find an element with the text: No agents are configured yet.`
(the current text runs on into the title-bar sentence).

- [ ] **Step 3: Add the button in `src/components/assistant/AssistantStartView.tsx`**

Add `Settings` to the lucide import (between `Play` and `ShieldCheck`) and
`import { useSettingsStore } from '@/stores/settings-store';` after the `assistant-store` import.
In the component, after `const { data: agentConfigs = [] } = useAgentConfigs();` (line 33), add:

```tsx
  const openSettings = useSettingsStore((s) => s.openSettings);
```

Replace (lines 108-112):

```tsx
      {agentConfigs.length === 0 ? (
        <p className='text-xs text-muted-foreground'>
          No agents are configured yet. Add one with the agent button in the title bar.
        </p>
      ) : (
```

with:

```tsx
      {agentConfigs.length === 0 ? (
        <div className='flex flex-col items-start gap-2'>
          <p className='text-xs text-muted-foreground'>No agents are configured yet.</p>
          <Button variant='outline' size='sm' onClick={() => openSettings('ai')}>
            <Settings className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
            Open AI Agents settings
          </Button>
        </div>
      ) : (
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/assistant/__tests__/AssistantStartView-no-agents.test.tsx src/components/assistant/__tests__/AssistantStartView.test.tsx`
Expected: PASS (1 new test; the 5 existing retry tests unchanged).

- [ ] **Step 5: Write `docs/features/settings-page.md`**

```markdown
# Settings page

One global Settings page, modelled on Bruno's Preferences. Spec:
`docs/superpowers/specs/2026-10-31-settings-page-shell-design.md` (parity map and later
sub-projects R-73 to R-77). Plans: `docs/superpowers/plans/settings-page-shell/`.

## Where it is

- The title-bar gear and Ctrl/Cmd+, open and close it. It covers the editor area only; the title
  bar, sidebar, assistant panel and status bar stay usable.
- The tabs underneath stay mounted and `inert` (`src/components/layout/MainArea.tsx`), so drafts,
  sessions and editors come back unchanged.
- It is global: a workspace or collection switch keeps it open. It is not saved in UI state.

## Sections

Order follows Bruno: Proxy, Secret Managers, AI Agents, Support, About. The list is
`SETTINGS_SECTIONS` in `src/components/settings/settings-sections.ts`; section components live in
`src/components/settings/sections/`.

To add a section: add its id to `SettingsSection` in `src/stores/settings-store.ts`, add a row to
`SETTINGS_SECTIONS` at its Bruno position (General, Themes and Display before Proxy; Client
Certificates after Secret Managers; Keybindings before AI Agents; Cache and Beta after Support), and
start the component with `SettingsSectionHeader`. Simple settings apply at once; forms that can be
invalid keep a Save button.

## State and rules

- `useSettingsStore` (`src/stores/settings-store.ts`): `open`, `section`, `locked`.
- Closing: Close button, Esc (not from a field, combobox or nested dialog), Ctrl/Cmd+W, Ctrl/Cmd+,,
  the gear, and any `openTab` or `openCollectionTab` call.
- While open, Ctrl/Cmd+Enter and Ctrl/Cmd+S do nothing, so they never act on a hidden tab.
- `locked` blocks every way out and every section switch. AI Agents holds it from Save until the
  agent and its credential are written, and releases it on unmount. A tab opened while locked
  opens underneath; Settings stays on top until the save ends.
- A section's form state resets when it unmounts (leaving the section or closing Settings).
- Proxy: Save stays on the section and reloads the stored setting; Reload drops edits or retries a
  failed load.

## Manual checks

1. Gear and Ctrl+, open and close Settings; an unsaved request draft and a connected WebSocket are
   unchanged afterwards.
2. Proxy: load, edit, Save, Reload; a request uses the new proxy.
3. Secret Managers: add, test, edit and delete a connection; an environment's External Secrets tab
   sees the change.
4. AI Agents: add an agent with a keychain credential; during the save, Close, Esc, Ctrl+W, the
   gear and a sidebar click do not leave Settings.
5. A sidebar click closes Settings and shows the request.
6. A workspace switch keeps Settings open.
7. Support links open the browser; About shows the version and the data folder.
8. With no agents, the assistant's "Open AI Agents settings" button opens the AI Agents section.

## Follow-ups

- R-73 Themes and Display, R-74 General, R-75 global client certificates, R-76 Keybindings,
  R-77 Cache and Beta (`docs/roadmaps/roadmap.md`, section 8f).
- The theme toggle still lives in the status bar until R-73 decides where it goes.
```

- [ ] **Step 6: Add the `CLAUDE.md` pointer**

After the line
``See `docs/features/ipc-command-acl.md` for the app command ACL (app-commands.txt, main-window-only access).``
insert:

```markdown
See `docs/features/settings-page.md` for the global Settings page (sections, lock, shortcuts, how to add a section).
```

- [ ] **Step 7: Update the roadmap**

In `docs/roadmaps/roadmap.md`, row R-72: set Status to `done (<first>..<last>)` using the first and
last commit hashes of this series (`git log --oneline` since the spec commit `05147c54`), set Where
to `spec above; plans docs/superpowers/plans/settings-page-shell/; docs/features/settings-page.md`,
and set What remains to `Manual checks in docs/features/settings-page.md.`

- [ ] **Step 8: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add src/components/assistant/AssistantStartView.tsx \
  src/components/assistant/__tests__/AssistantStartView-no-agents.test.tsx \
  docs/features/settings-page.md CLAUDE.md docs/roadmaps/roadmap.md
```

Commit through `dev-workflow-skills:1-git-commit`. Suggested subject:
`feat(assistant): open AI agent settings from the start view`. Put the docs in the same commit or,
if the skill proposes a split, as `docs(settings): document the settings page`.

---

## End of Plan 3. Verify

- `yarn tsc --noEmit` and `yarn check` pass.
- `yarn test src/components/settings src/components/title-bar src/components/assistant src/components/layout src/components/environments src/hooks/__tests__ src/stores/__tests__/settings-store.test.ts src/stores/__tests__/pane-store-settings.test.ts src/stores/__tests__/pane-store.test.ts` passes.
- `grep -rn "AgentConfigsDialog\|ProxySettingsDialog\|SecretManagerConnectionsDialog\|title bar" src/components --include='*.tsx' | grep -v __tests__` shows no user-facing "title bar" hint for settings.
- Manual checks 1-8 in `docs/features/settings-page.md` (`yarn tauri dev`).
- Then run the final whole-branch review (Opus) over the three plans' commits.

## Self-Review

- Spec coverage across the series: 4.1 (plan 01 T3), 4.2 (plan 01 T1), 4.3 (plan 01 T2/T3,
  plan 02 T1, plan 03 T2), 4.4 (plan 02 T1), 4.5 (plan 01 T2, rows in 02 T2/T3 and 03 T1), 4.6
  (plan 02 T2/T3, plan 03 T1), 4.7 (plan 01 T2), 4.8 (plan 02 T2/T3, plan 03 T1/T2), 4.9 out of
  scope (nothing planned), 5 (plan 01 T2, plan 02 T2), 6 (tests in every task; manual checks here).
- Names used across plans: `useSettingsStore`, `SettingsSection` (`'proxy' | 'secret-managers' |
  'ai' | 'support' | 'about'`), `SETTINGS_SECTIONS`, `SettingsSectionDef`, `resolveSection`,
  `SettingsSectionHeader`, `SettingsPage`, `MainArea`, `ProxySection`, `SecretManagersSection`,
  `AiAgentsSection`, `SupportSection`, `AboutSection`.
- Not covered by automated tests: real browser opening from Support, the keychain write in the
  real OS, Monaco and WebSocket survival in the real webview (manual checks 1, 4 and 7).
