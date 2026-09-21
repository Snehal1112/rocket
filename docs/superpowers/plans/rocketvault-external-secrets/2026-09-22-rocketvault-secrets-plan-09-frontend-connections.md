# RocketVault Secrets Plan 09: Frontend Types + Secret Manager Connections UI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the TypeScript types mirroring the Rust IPC DTOs from Plan 08, thin
wrapper functions for the five new Tauri commands, and this app's first
app-level Preferences surface: a dialog for managing named RocketVault
connections, reachable from a new icon in the title bar.

**Architecture:** `src/lib/tauri-api.ts` gains the new types and wrapper
functions in the same style as every existing entry in that file. A new
`src/lib/queries/secret-manager-queries.ts` follows the TanStack Query hook
pattern already established by `src/lib/queries/environment-queries.ts`. A new
`src/components/settings/` directory (first of its kind in this codebase — no
app-level Preferences surface exists yet) holds
`SecretManagerConnectionsDialog.tsx`, modeled directly on
`src/components/git/GitCredentialsDialog.tsx`'s form/save/error pattern, but
using local `open`/`onOpenChange` props (a plain controlled `Dialog`) rather
than `GitCredentialsDialog`'s Zustand-store-driven visibility, since this
dialog has no equivalent existing store and doesn't need one — it's opened
from one fixed place (the title bar).

**Tech Stack:** TypeScript, React, TanStack Query, shadcn/ui, `sonner` (toasts), Vitest, Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.3, §4.5). Plan index: `docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md`
(locked interface contract). Depends on Plan 08's five Tauri commands
(`list_secret_manager_connections`, `save_secret_manager_connection`,
`delete_secret_manager_connection`, `test_secret_manager_connection`,
`fetch_external_secret_names`).

## Global Constraints

- All new types use camelCase field names (IPC boundary) — matches every
  existing interface in `tauri-api.ts`, e.g. `Variable`/`Environment`.
- All UI uses shadcn/ui primitives (`src/components/ui/`) and `lucide-react`
  icons only — no raw `<button>`/`<input>`/`<dialog>` elements, per this
  repo's hard rules.
- Data fetching uses TanStack Query hooks (`src/lib/queries/`), not raw
  `useEffect`/`useState` fetching, matching `environment-queries.ts`'s
  established pattern for this codebase.
- Errors surface via `toast.error(...)` from `sonner`, matching
  `EnvironmentDialog.tsx`'s existing convention.
- A connection's client secret is write-only from the frontend's perspective:
  the backend never returns it (Plan 08's `SecretManagerConnectionDto` has no
  secret field at all), so the secret input is always blank when editing an
  existing connection, and saving with it left blank must not clear the
  stored secret (`clientSecret: undefined` on save, per Plan 05's "`None`
  leaves the existing keychain entry untouched" semantics).

---

## Task 1: TS types + `tauri-api.ts` wrapper functions

**Files:**
- Modify: `src/lib/tauri-api.ts`

**Interfaces:**
- Produces: `ExternalSecretRef`, `ExternalSecretBinding`, `SecretManagerConnection`
  types; updated `Environment` interface; `listSecretManagerConnections`,
  `saveSecretManagerConnection`, `deleteSecretManagerConnection`,
  `testSecretManagerConnection`, `fetchExternalSecretNames` wrapper functions
  — consumed by Task 2 of this plan (via `secret-manager-queries.ts`) and by
  Plan 10 (Environment External Secrets tab).

- [ ] **Step 1: Add the types**

In `src/lib/tauri-api.ts`, near the existing `Variable`/`Environment`
interfaces (around line 164), add:

```typescript
export interface ExternalSecretRef {
  name: string;
  secretId: string;
}

export interface ExternalSecretBinding {
  alias: string;
  connectionId: string;
  vaultName: string;
  secretNames: ExternalSecretRef[];
}

export interface SecretManagerConnection {
  id: string;
  label: string;
  baseUrl: string;
  clientId: string;
  verifySsl: boolean;
  allowInsecureHttp: boolean;
}
```

Update the existing `Environment` interface to add the new field:

```typescript
export interface Environment {
  name: string;
  variables: Variable[];
  externalSecrets: ExternalSecretBinding[];
}
```

This changes `Environment`'s shape, which several existing call sites
construct object literals against (e.g. environment-creation code in
`EnvironmentDialog.tsx` or its store). Search for every place that builds an
`Environment` object literal (`grep -rn "externalSecrets\|: Environment = {\|Environment>({" src/` as a starting point, but the more reliable check is
just running the TypeScript compiler after this change and fixing every error
it reports) and add `externalSecrets: []` to each one so the codebase keeps
compiling — do this as part of this step, not deferred to a later task, since
an uncompilable intermediate state defeats the point of task-by-task commits.

- [ ] **Step 2: Add the wrapper functions**

Add a new section near the end of the file (after the "Security audit /
compliance" section, following the file's existing `// ====...====` section
divider convention):

```typescript
// ============================================================
// Secret Manager connections (RocketVault external secrets)
// ============================================================

export const listSecretManagerConnections = () =>
  invoke<SecretManagerConnection[]>('list_secret_manager_connections');

export const saveSecretManagerConnection = (
  connection: SecretManagerConnection,
  clientSecret?: string,
) =>
  invoke<void>('save_secret_manager_connection', {
    connection,
    clientSecret,
  });

export const deleteSecretManagerConnection = (id: string) =>
  invoke<void>('delete_secret_manager_connection', { id });

export const testSecretManagerConnection = (id: string, vaultName: string) =>
  invoke<void>('test_secret_manager_connection', { id, vaultName });

export const fetchExternalSecretNames = (id: string, vaultName: string) =>
  invoke<ExternalSecretRef[]>('fetch_external_secret_names', { id, vaultName });
```

(This matches every existing wrapper's style in this file — a thin arrow
function calling `invoke` with a typed generic and a snake_case Tauri command
name paired with a camelCase argument object, e.g. compare to
`saveEnvironment`/`deleteEnvironment` a few sections above.)

- [ ] **Step 3: Verify the crate compiles**

Run: `yarn tsc --noEmit`
Expected: PASS — no type errors, including at every `Environment` literal
site fixed in Step 1.

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(frontend): add Secret Manager connection types and wrappers"
```

---

## Task 2: `secret-manager-queries.ts` + `SecretManagerConnectionsDialog.tsx`

**Files:**
- Create: `src/lib/queries/secret-manager-queries.ts`
- Create: `src/components/settings/SecretManagerConnectionsDialog.tsx`
- Create: `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx`

**Interfaces:**
- Consumes: Task 1's types and wrapper functions.
- Produces: `useSecretManagerConnections()` query hook,
  `SecretManagerConnectionsDialog({ open, onOpenChange }: { open: boolean;
  onOpenChange: (open: boolean) => void })` — consumed by Task 3 of this plan
  (Task 3 wires it into `TitleBar.tsx`).

- [ ] **Step 1: Query hooks**

Create `src/lib/queries/secret-manager-queries.ts`, following
`environment-queries.ts`'s exact pattern (query keys object, `useQuery`/
`useMutation` with `useQueryClient` for invalidation):

```typescript
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  deleteSecretManagerConnection,
  listSecretManagerConnections,
  saveSecretManagerConnection,
  type SecretManagerConnection,
  testSecretManagerConnection,
} from '@/lib/tauri-api';

export const secretManagerKeys = {
  list: ['secretManagerConnections'] as const,
};

export function useSecretManagerConnections() {
  return useQuery({
    queryKey: secretManagerKeys.list,
    queryFn: listSecretManagerConnections,
  });
}

export function useSaveSecretManagerConnection() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      connection,
      clientSecret,
    }: {
      connection: SecretManagerConnection;
      clientSecret?: string;
    }) => saveSecretManagerConnection(connection, clientSecret),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: secretManagerKeys.list });
    },
  });
}

export function useDeleteSecretManagerConnection() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteSecretManagerConnection(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: secretManagerKeys.list });
    },
  });
}

export function useTestSecretManagerConnection() {
  return useMutation({
    mutationFn: ({ id, vaultName }: { id: string; vaultName: string }) =>
      testSecretManagerConnection(id, vaultName),
  });
}
```

- [ ] **Step 2: `SecretManagerConnectionsDialog.tsx`**

Create `src/components/settings/SecretManagerConnectionsDialog.tsx`. List +
add/edit form in one dialog (a simple `editingId: string | null` local state
toggles between "list view" and "form view", avoiding a second nested
dialog):

```typescript
import { useState } from 'react';
import { toast } from 'sonner';
import { Loader2, Pencil, Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  useDeleteSecretManagerConnection,
  useSaveSecretManagerConnection,
  useSecretManagerConnections,
  useTestSecretManagerConnection,
} from '@/lib/queries/secret-manager-queries';
import type { SecretManagerConnection } from '@/lib/tauri-api';

interface SecretManagerConnectionsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const emptyForm = {
  id: '',
  label: '',
  baseUrl: '',
  clientId: '',
  verifySsl: true,
  allowInsecureHttp: false,
  clientSecret: '',
};

export function SecretManagerConnectionsDialog({
  open,
  onOpenChange,
}: SecretManagerConnectionsDialogProps) {
  const { data: connections = [] } = useSecretManagerConnections();
  const saveMutation = useSaveSecretManagerConnection();
  const deleteMutation = useDeleteSecretManagerConnection();
  const testMutation = useTestSecretManagerConnection();

  const [editing, setEditing] = useState<typeof emptyForm | null>(null);
  const [testVaultName, setTestVaultName] = useState('');

  const startAdd = () => setEditing({ ...emptyForm, id: crypto.randomUUID() });
  const startEdit = (c: SecretManagerConnection) =>
    setEditing({ ...c, clientSecret: '' });

  const handleSave = async () => {
    if (!editing) return;
    const connection: SecretManagerConnection = {
      id: editing.id,
      label: editing.label,
      baseUrl: editing.baseUrl,
      clientId: editing.clientId,
      verifySsl: editing.verifySsl,
      allowInsecureHttp: editing.allowInsecureHttp,
    };
    try {
      await saveMutation.mutateAsync({
        connection,
        clientSecret: editing.clientSecret || undefined,
      });
      setEditing(null);
    } catch (e) {
      toast.error(`Could not save connection: ${String(e)}`);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteMutation.mutateAsync(id);
    } catch (e) {
      toast.error(`Could not delete connection: ${String(e)}`);
    }
  };

  const handleTest = async (id: string) => {
    if (!testVaultName) {
      toast.error('Enter a vault name to test against.');
      return;
    }
    try {
      await testMutation.mutateAsync({ id, vaultName: testVaultName });
      toast.success('Connection succeeded.');
    } catch (e) {
      toast.error(`Connection failed: ${String(e)}`);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='w-auto min-w-[28rem] max-w-[min(90vw,_48rem)]'>
        <DialogHeader>
          <DialogTitle>Secret Manager Connections</DialogTitle>
        </DialogHeader>

        {editing ? (
          <div className='space-y-3'>
            <div>
              <Label htmlFor='sm-label' className='text-sm'>
                Label
              </Label>
              <Input
                id='sm-label'
                value={editing.label}
                onChange={(e) => setEditing({ ...editing, label: e.target.value })}
                className='h-8 text-sm'
              />
            </div>
            <div>
              <Label htmlFor='sm-base-url' className='text-sm'>
                Base URL
              </Label>
              <Input
                id='sm-base-url'
                value={editing.baseUrl}
                onChange={(e) => setEditing({ ...editing, baseUrl: e.target.value })}
                placeholder='https://vault.internal:8774'
                className='h-8 text-sm'
              />
            </div>
            <div>
              <Label htmlFor='sm-client-id' className='text-sm'>
                Client ID
              </Label>
              <Input
                id='sm-client-id'
                value={editing.clientId}
                onChange={(e) => setEditing({ ...editing, clientId: e.target.value })}
                className='h-8 text-sm'
              />
            </div>
            <div>
              <Label htmlFor='sm-client-secret' className='text-sm'>
                Client Secret{' '}
                <span className='text-muted-foreground'>
                  (leave blank to keep the existing secret)
                </span>
              </Label>
              <Input
                id='sm-client-secret'
                type='password'
                value={editing.clientSecret}
                onChange={(e) => setEditing({ ...editing, clientSecret: e.target.value })}
                className='h-8 text-sm'
                autoComplete='new-password'
              />
            </div>
            <div className='flex items-center justify-between'>
              <Label htmlFor='sm-verify-ssl' className='text-sm'>
                Verify SSL
              </Label>
              <Switch
                id='sm-verify-ssl'
                checked={editing.verifySsl}
                onCheckedChange={(checked) => setEditing({ ...editing, verifySsl: checked })}
              />
            </div>
            <div className='flex items-center justify-between'>
              <Label htmlFor='sm-allow-insecure' className='text-sm'>
                Allow insecure HTTP{' '}
                <span className='text-muted-foreground'>(loopback only recommended)</span>
              </Label>
              <Switch
                id='sm-allow-insecure'
                checked={editing.allowInsecureHttp}
                onCheckedChange={(checked) =>
                  setEditing({ ...editing, allowInsecureHttp: checked })
                }
              />
            </div>
            <div className='flex gap-2'>
              <Button variant='outline' size='sm' onClick={() => setEditing(null)}>
                Cancel
              </Button>
              <Button
                size='sm'
                onClick={() => void handleSave()}
                disabled={saveMutation.isPending}
                aria-busy={saveMutation.isPending}
              >
                {saveMutation.isPending && <Loader2 className='h-3.5 w-3.5 animate-spin' />}
                Save
              </Button>
            </div>
          </div>
        ) : (
          <div className='space-y-3'>
            {connections.length === 0 && (
              <p className='text-sm text-muted-foreground'>No connections configured.</p>
            )}
            {connections.map((c) => (
              <div key={c.id} className='flex items-center justify-between gap-2 text-sm'>
                <div>
                  <div className='font-medium'>{c.label}</div>
                  <div className='text-xs text-muted-foreground'>{c.baseUrl}</div>
                </div>
                <div className='flex items-center gap-1'>
                  <Input
                    value={testVaultName}
                    onChange={(e) => setTestVaultName(e.target.value)}
                    placeholder='vault name'
                    className='h-7 w-28 text-xs'
                  />
                  <Button
                    variant='outline'
                    size='sm'
                    onClick={() => void handleTest(c.id)}
                    disabled={testMutation.isPending}
                  >
                    Test
                  </Button>
                  <Button
                    variant='ghost'
                    size='icon'
                    aria-label='Edit connection'
                    onClick={() => startEdit(c)}
                  >
                    <Pencil className='h-3.5 w-3.5' aria-hidden='true' />
                  </Button>
                  <Button
                    variant='ghost'
                    size='icon'
                    aria-label='Delete connection'
                    onClick={() => void handleDelete(c.id)}
                  >
                    <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
                  </Button>
                </div>
              </div>
            ))}
            <Button size='sm' onClick={startAdd}>
              <Plus className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
              Add Connection
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
```

(`src/components/ui/switch.tsx` already exists in this codebase — confirmed,
`Switch` is a valid existing shadcn primitive here, not a new pattern.)

- [ ] **Step 3: Component test**

Create `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx`,
mirroring `GitCredentialsDialog.test.tsx`'s mocking style (`vi.mock('@/lib/tauri-api', ...)`)
but wrapped in a `QueryClientProvider` (check how any existing TanStack-Query-consuming
component test sets this up, e.g. search for `QueryClientProvider` in
`src/components/**/__tests__/` and match that exact test-harness setup):

```typescript
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SecretManagerConnectionsDialog } from '@/components/settings/SecretManagerConnectionsDialog';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listSecretManagerConnections: vi.fn().mockResolvedValue([]),
    saveSecretManagerConnection: vi.fn().mockResolvedValue(undefined),
    deleteSecretManagerConnection: vi.fn().mockResolvedValue(undefined),
    testSecretManagerConnection: vi.fn().mockResolvedValue(undefined),
  };
});

function renderDialog() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <SecretManagerConnectionsDialog open onOpenChange={() => {}} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(tauriApi.listSecretManagerConnections).mockResolvedValue([]);
});

describe('SecretManagerConnectionsDialog', () => {
  it('renders an empty state with no connections', async () => {
    renderDialog();
    expect(await screen.findByText(/no connections configured/i)).toBeInTheDocument();
  });

  it('adding a connection calls save with the right shape and no client secret when left blank', async () => {
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.type(screen.getByLabelText(/^label$/i), 'Prod Vault');
    await user.type(screen.getByLabelText(/base url/i), 'https://vault.internal:8774');
    await user.type(screen.getByLabelText(/client id/i), 'rocketapi');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).toHaveBeenCalledWith(
      expect.objectContaining({ label: 'Prod Vault', baseUrl: 'https://vault.internal:8774' }),
      undefined,
    );
  });

  it('editing an existing connection without touching the secret field saves with clientSecret undefined', async () => {
    vi.mocked(tauriApi.listSecretManagerConnections).mockResolvedValue([
      {
        id: 'conn-1',
        label: 'Prod',
        baseUrl: 'https://vault.internal:8774',
        clientId: 'rocketapi',
        verifySsl: true,
        allowInsecureHttp: false,
      },
    ]);
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /edit connection/i }));
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'conn-1' }),
      undefined,
    );
  });
});
```

(The edit-button query above relies on Step 2's `aria-label='Edit connection'`
on the icon-only edit `Button` — an icon-only button needs an `aria-label` to
have a meaningful, accessible name at all, which is why Step 2 already
includes it.)

- [ ] **Step 4: Run tests**

Run: `yarn test SecretManagerConnectionsDialog`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/queries/secret-manager-queries.ts src/components/settings/
git commit -m "feat(frontend): add Secret Manager Connections dialog"
```

---

## Task 3: Entry point in the title bar

**Files:**
- Modify: `src/components/title-bar/TitleBar.tsx`

**Interfaces:**
- Consumes: `SecretManagerConnectionsDialog` from Task 2.

- [ ] **Step 1: Add the icon button and dialog**

`TitleBar.tsx` is currently:

```typescript
import { type as osType } from '@tauri-apps/plugin-os';
import { WindowControls } from './WindowControls';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export function TitleBar() {
  const isMac = osType() === 'macos';

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

      <div className='flex items-center justify-end h-full' data-tauri-drag-region>
        {!isMac && <WindowControls />}
      </div>
    </div>
  );
}
```

Re-read the actual current file before editing — it may have changed since
this plan was written. Change it to:

```typescript
import { type as osType } from '@tauri-apps/plugin-os';
import { Settings } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { SecretManagerConnectionsDialog } from '@/components/settings/SecretManagerConnectionsDialog';
import { WindowControls } from './WindowControls';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export function TitleBar() {
  const isMac = osType() === 'macos';
  const [showSecretManagers, setShowSecretManagers] = useState(false);

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
        {!isMac && <WindowControls />}
      </div>

      <SecretManagerConnectionsDialog
        open={showSecretManagers}
        onOpenChange={setShowSecretManagers}
      />
    </div>
  );
}
```

(The new `Button` sits inside a `data-tauri-drag-region` container — confirm
by checking `WindowControls`'s own rendering, or this repo's existing
pattern elsewhere, that a `Button` inside a drag region still receives click
events correctly in Tauri; if not, wrap just the `Button` in a
non-drag-region `<div>` to be safe, matching whatever the codebase already
does for interactive elements placed inside a `data-tauri-drag-region`
ancestor — check `WorkspaceSwitcher.tsx` for precedent, since it already
renders interactive controls inside this same title bar.)

- [ ] **Step 2: Test (if a `TitleBar` test file already exists)**

Check for `src/components/title-bar/__tests__/TitleBar.test.tsx` or similar.
If one exists, add a test asserting the new button renders and clicking it
opens the dialog (`screen.getByRole('button', { name: /secret manager connections/i })`,
then assert the dialog's title text appears after a click). If no test file
exists for `TitleBar` today, note explicitly in this step that a new test
file is being deliberately skipped for this trivial wiring change (this is a
one-line behavioral addition to an otherwise-untested presentational
component) rather than silently omitted — do not invent a new
`TitleBar.test.tsx` file as unrequested scope expansion if this component has
no existing test coverage to extend.

- [ ] **Step 3: Run the full frontend check**

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/components/title-bar/TitleBar.tsx
git commit -m "feat(frontend): add Secret Manager connections entry point to title bar"
```

---

## Milestone Checklist — Plan 09

- [ ] `ExternalSecretRef`/`ExternalSecretBinding`/`SecretManagerConnection` TS types match the locked contract exactly
- [ ] `Environment` interface gains `externalSecrets`, every existing literal site fixed
- [ ] Five wrapper functions in `tauri-api.ts` calling the Plan 08 commands
- [ ] `secret-manager-queries.ts` — TanStack Query hooks following `environment-queries.ts`'s pattern
- [ ] `SecretManagerConnectionsDialog.tsx` — list/add/edit/delete/test, secret field never pre-filled on edit
- [ ] Title bar icon button opens the dialog
- [ ] `yarn tsc --noEmit` and `yarn test` pass

## Next Plan

[Plan 10: Frontend Environment External Secrets tab](2026-09-22-rocketvault-secrets-plan-10-frontend-external-secrets-tab.md) —
consumes this plan's TS types (`ExternalSecretBinding`/`ExternalSecretRef`)
and the `useSecretManagerConnections` query hook to populate a connection
picker inside the environment editor.
