# ACP Agent Config Plan 05: Frontend UI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the "AI Agents" settings dialog — list/add/edit/delete/test
configured ACP agents — consuming Plan 04's four Tauri commands.

**Architecture:** A standalone dialog component, `AgentConfigsDialog.tsx`,
opened from its own icon button in `TitleBar`, structurally identical to the
existing `SecretManagerConnectionsDialog`/`TitleBar` pair — this repo
currently has no multi-section Settings shell to plug a new tab into, so a
sibling dialog is the change that actually matches what exists today, not a
speculative Settings-shell redesign.

**Tech Stack:** React, TypeScript, TanStack Query, shadcn/ui, lucide-react,
Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-27-acp-agent-config-credentials-design.md`
(Frontend UI section — note: the spec's mention of `SingleLineEditor` fields
is superseded below; the actual precedent component for this exact kind of
settings form, `SecretManagerConnectionsDialog`, uses plain shadcn `Input`
for its label/URL/ID fields, since none of them are `{{variable}}`-template-
aware — this plan follows the real precedent). Plan index:
`docs/superpowers/plans/acp-agent-config-credentials/00-plan-index.md`.

## Global Constraints

- Follow `SecretManagerConnectionsDialog.tsx`
  (`src/components/settings/SecretManagerConnectionsDialog.tsx`) as the
  structural template: a single dialog with two states — a list view and an
  edit-form view swapped via local `editing` state, not a separate
  add/edit modal.
- Use shadcn `Input`/`Label`/`Button`/`Dialog`/`Switch` primitives and
  `lucide-react` icons only — no raw `<input>`/`<button>` elements, per this
  repo's hard rule.
- `AgentConfigDto`'s Rust field names are already camelCase at the IPC
  boundary (Plan 04); the TypeScript `AgentConfig` interface uses the same
  camelCase names directly — no further renaming layer needed, matching how
  `SecretManagerConnection` (`src/lib/tauri-api.ts:192-199`) mirrors its DTO.
- Query hook mutations invalidate the list query key on success, matching
  `useSaveSecretManagerConnection`/`useDeleteSecretManagerConnection`
  (`src/lib/queries/secret-manager-queries.ts:21-45`) exactly.

## Review Focus

- Saving a new agent config with a blank `label`/`command` must be rejected
  client-side before the IPC call, mirroring
  `SecretManagerConnectionsDialog`'s existing pre-submit validation
  (`src/components/settings/SecretManagerConnectionsDialog.tsx:64-77`) —
  don't rely on the backend's `DomainError::InvalidInput` alone for basic
  required-field feedback.
- The vault-secret picker must handle "no vault connections configured yet"
  gracefully (empty state with a hint to add one via the existing Secret
  Manager Connections dialog first), not a blank/broken dropdown.
- `testAgentConfig`'s result must surface both the "command not found" and
  "credential no longer exists in RocketVault" failure messages distinctly to
  the user (both are plain `DomainError` messages from Plan 03) — a generic
  "test failed" toast would hide which of the two problems occurred.
- Deleting an agent config currently in use is out of scope for this plan (no
  later subproject exists yet to be "using" one) — no confirmation-of-impact
  step is needed beyond the existing delete-confirmation row pattern already
  used by `SecretManagerConnectionsDialog`.
- The dialog must reset its `editing`/local state on close (`open` becoming
  `false`), matching `SecretManagerConnectionsDialog`'s existing `useEffect`
  (`src/components/settings/SecretManagerConnectionsDialog.tsx:52-58`) — an
  agent's in-progress edit must not leak into the next time the dialog opens.

---

## Task 1: Types, API wrappers, and query hooks

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Create: `src/lib/queries/agent-config-queries.ts`
- Create: `src/lib/queries/__tests__/agent-config-queries.test.tsx`

**Interfaces:**
- Consumes: `list_agent_configs`/`save_agent_config`/`delete_agent_config`/
  `test_agent_config` Tauri commands (Plan 04).
- Produces: `AgentConfig` TypeScript interface,
  `listAgentConfigs`/`saveAgentConfig`/`deleteAgentConfig`/`testAgentConfig`
  functions, `useAgentConfigs`/`useSaveAgentConfig`/`useDeleteAgentConfig`/
  `useTestAgentConfig` hooks — consumed by Task 2 of this plan.

- [ ] **Step 1: Write the failing test**

```typescript
// src/lib/queries/__tests__/agent-config-queries.test.tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listAgentConfigs: vi.fn(),
    saveAgentConfig: vi.fn(),
    deleteAgentConfig: vi.fn(),
    testAgentConfig: vi.fn(),
  };
});

let queryClient: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

const sampleConfig: tauriApi.AgentConfig = {
  id: 'agent-1',
  label: 'Claude Agent',
  command: 'claude-agent-acp',
  args: ['--stdio'],
  workingDir: undefined,
  credentialEnvVar: 'ANTHROPIC_API_KEY',
  vaultConnectionId: 'conn-1',
  vaultName: 'prod-vault',
  vaultSecretId: 'secret-id-1',
  vaultSecretName: 'anthropic-api-key',
};

describe('agent config queries', () => {
  beforeEach(() => {
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    vi.mocked(tauriApi.listAgentConfigs).mockResolvedValue([sampleConfig]);
    vi.mocked(tauriApi.saveAgentConfig).mockResolvedValue(undefined);
    vi.mocked(tauriApi.deleteAgentConfig).mockResolvedValue(undefined);
    vi.mocked(tauriApi.testAgentConfig).mockResolvedValue(undefined);
  });

  it('useAgentConfigs fetches the list', async () => {
    const { useAgentConfigs } = await import('../agent-config-queries');
    const { result } = renderHook(() => useAgentConfigs(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual([sampleConfig]));
  });

  it('useSaveAgentConfig calls saveAgentConfig and invalidates the list', async () => {
    const { useAgentConfigs, useSaveAgentConfig } = await import('../agent-config-queries');
    const { result: list } = renderHook(() => useAgentConfigs(), { wrapper });
    await waitFor(() => expect(list.current.data).toEqual([sampleConfig]));

    const { result: save } = renderHook(() => useSaveAgentConfig(), { wrapper });
    await save.current.mutateAsync(sampleConfig);

    expect(tauriApi.saveAgentConfig).toHaveBeenCalledWith(sampleConfig);
    expect(tauriApi.listAgentConfigs).toHaveBeenCalledTimes(2);
  });

  it('useDeleteAgentConfig calls deleteAgentConfig with the id', async () => {
    const { useDeleteAgentConfig } = await import('../agent-config-queries');
    const { result } = renderHook(() => useDeleteAgentConfig(), { wrapper });
    await result.current.mutateAsync('agent-1');
    expect(tauriApi.deleteAgentConfig).toHaveBeenCalledWith('agent-1');
  });

  it('useTestAgentConfig calls testAgentConfig with the id', async () => {
    const { useTestAgentConfig } = await import('../agent-config-queries');
    const { result } = renderHook(() => useTestAgentConfig(), { wrapper });
    await result.current.mutateAsync('agent-1');
    expect(tauriApi.testAgentConfig).toHaveBeenCalledWith('agent-1');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/queries/__tests__/agent-config-queries.test.tsx`
Expected: FAIL — `AgentConfig`, `listAgentConfigs`, etc. do not exist in
`@/lib/tauri-api` yet.

- [ ] **Step 3: Add the type and API wrappers**

In `src/lib/tauri-api.ts`, add the interface alongside the existing
`SecretManagerConnection` interface (around line 199):

```typescript
export interface AgentConfig {
  id: string;
  label: string;
  command: string;
  args: string[];
  workingDir?: string;
  credentialEnvVar: string;
  vaultConnectionId: string;
  vaultName: string;
  vaultSecretId: string;
  vaultSecretName: string;
}
```

Add the wrapper functions in the "Secret Manager connections" section (near
the end of the file, around line 1700), in a new block right after
`fetchExternalSecretNames`:

```typescript
// ============================================================
// Agent configs (ACP AI assist)
// ============================================================

export const listAgentConfigs = () => invoke<AgentConfig[]>('list_agent_configs');

export const saveAgentConfig = (config: AgentConfig) =>
  invoke<void>('save_agent_config', { config });

export const deleteAgentConfig = (id: string) => invoke<void>('delete_agent_config', { id });

export const testAgentConfig = (id: string) => invoke<void>('test_agent_config', { id });
```

- [ ] **Step 4: Add the query hooks**

Create `src/lib/queries/agent-config-queries.ts`:

```typescript
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  type AgentConfig,
  deleteAgentConfig,
  listAgentConfigs,
  saveAgentConfig,
  testAgentConfig,
} from '@/lib/tauri-api';

export const agentConfigKeys = {
  list: ['agentConfigs'] as const,
};

export function useAgentConfigs() {
  return useQuery({
    queryKey: agentConfigKeys.list,
    queryFn: listAgentConfigs,
  });
}

export function useSaveAgentConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (config: AgentConfig) => saveAgentConfig(config),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: agentConfigKeys.list });
    },
  });
}

export function useDeleteAgentConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteAgentConfig(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: agentConfigKeys.list });
    },
  });
}

export function useTestAgentConfig() {
  return useMutation({
    mutationFn: (id: string) => testAgentConfig(id),
  });
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `yarn vitest run src/lib/queries/__tests__/agent-config-queries.test.tsx`
Expected: PASS — 4 tests. Also run `yarn tsc --noEmit` to confirm the new
type/functions are consistent across the codebase.

- [ ] **Step 6: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/queries/agent-config-queries.ts src/lib/queries/__tests__/agent-config-queries.test.tsx
git commit -m "feat(frontend): add agent config API and query hooks"
```

---

## Task 2: `AgentConfigsDialog` and `TitleBar` wiring

**Files:**
- Create: `src/components/settings/AgentConfigsDialog.tsx`
- Modify: `src/components/title-bar/TitleBar.tsx`

**Interfaces:**
- Consumes: `useAgentConfigs`/`useSaveAgentConfig`/`useDeleteAgentConfig`/
  `useTestAgentConfig` (Task 1 of this plan); `useSecretManagerConnections`
  (existing, for the vault-connection picker); `fetchExternalSecretNames`
  (existing, for the vault-secret picker).
- Produces: `AgentConfigsDialog` component, rendered from `TitleBar` — this is
  the terminal deliverable of subproject A; no later plan in this series
  consumes it further.

No unit test is added for this component: `SecretManagerConnectionsDialog`,
the component this plan mirrors exactly, has none either (confirmed — no
`SecretManagerConnectionsDialog.test.tsx` exists in this repo) — form-heavy
settings dialogs in this codebase are verified by `yarn tsc --noEmit` plus
manual exercise in the running app, not component tests. Adding one here
would be inventing a testing convention this plan's own template doesn't
follow, not filling a real gap.

- [ ] **Step 1: Build the dialog component**

Create `src/components/settings/AgentConfigsDialog.tsx`, structured exactly
like `SecretManagerConnectionsDialog.tsx` (list view / edit-form view swapped
by local `editing` state, per-row Test/Edit/Delete actions, delete
confirmation inline), with these agent-specific differences:

- Form fields: Label, Command, Args (comma-separated text input, split/joined
  on save), Working Directory (optional), Credential Env Var — all plain
  shadcn `Input`, matching the Global Constraints above.
- A vault picker section using shadcn `Select` (`@/components/ui/select` —
  `Select`/`SelectTrigger`/`SelectValue`/`SelectContent`/`SelectItem`; this
  repo's hard rule forbids a raw `<select>`), a plain `Input` for
  `vaultName`, and a "Fetch Secrets" button:

  ```tsx
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from '@/components/ui/select';
  import { fetchExternalSecretNames, type ExternalSecretRef } from '@/lib/tauri-api';
  import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';

  // inside the component:
  const { data: connections = [] } = useSecretManagerConnections();
  const [vaultSecrets, setVaultSecrets] = useState<ExternalSecretRef[]>([]);
  const [fetchingSecrets, setFetchingSecrets] = useState(false);

  const handleFetchSecrets = async () => {
    if (!editing || !editing.vaultConnectionId || !editing.vaultName.trim()) {
      toast.error('Select a connection and enter a vault name first.');
      return;
    }
    setFetchingSecrets(true);
    try {
      const secrets = await fetchExternalSecretNames(
        editing.vaultConnectionId,
        editing.vaultName.trim(),
      );
      setVaultSecrets(secrets);
      if (secrets.length === 0) {
        toast.error('No secrets found in that vault.');
      }
    } catch (e) {
      toast.error(`Could not fetch secrets: ${String(e)}`);
    } finally {
      setFetchingSecrets(false);
    }
  };

  // in the edit-form JSX:
  <Select
    value={editing.vaultConnectionId}
    onValueChange={(value) =>
      setEditing({ ...editing, vaultConnectionId: value, vaultSecretId: '', vaultSecretName: '' })
    }
  >
    <SelectTrigger className='h-8 text-sm'>
      <SelectValue placeholder='Select a connection…' />
    </SelectTrigger>
    <SelectContent>
      {connections.map((c) => (
        <SelectItem key={c.id} value={c.id}>
          {c.label}
        </SelectItem>
      ))}
    </SelectContent>
  </Select>
  <Input
    value={editing.vaultName}
    onChange={(e) => setEditing({ ...editing, vaultName: e.target.value })}
    placeholder='vault name'
    className='h-8 text-sm'
  />
  <Button
    variant='outline'
    size='sm'
    onClick={() => void handleFetchSecrets()}
    disabled={fetchingSecrets}
  >
    Fetch Secrets
  </Button>
  {vaultSecrets.length > 0 && (
    <Select
      value={editing.vaultSecretId}
      onValueChange={(value) => {
        const picked = vaultSecrets.find((s) => s.secretId === value);
        setEditing({ ...editing, vaultSecretId: value, vaultSecretName: picked?.name ?? '' });
      }}
    >
      <SelectTrigger className='h-8 text-sm'>
        <SelectValue placeholder='Select a secret…' />
      </SelectTrigger>
      <SelectContent>
        {vaultSecrets.map((s) => (
          <SelectItem key={s.secretId} value={s.secretId}>
            {s.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )}
  ```

  This mirrors the existing external-secret picker flow already proven in
  `ExternalSecretsTab.tsx` (fetch-then-pick against the same
  `fetchExternalSecretNames` call), adapted to shadcn `Select` here.
- If `connections` (from `useSecretManagerConnections()`) is empty, render an
  empty state directing the user to add a connection via the existing Secret
  Manager Connections dialog first, instead of an unusable empty `Select`
  (per this plan's Review Focus).
- The Test action calls `useTestAgentConfig().mutateAsync(id)` and surfaces
  the resolved/rejected `DomainError` message verbatim via `toast.success`/
  `toast.error` (mirroring `handleTest` in
  `SecretManagerConnectionsDialog.tsx:107-119`), so the two distinct failure
  messages from Plan 03 (`command not found` vs `credential no longer exists
  in RocketVault`) both reach the user distinctly.
- Client-side validation before save: reject blank `label`/`command`/
  `credentialEnvVar` and an unselected `vaultConnectionId`/`vaultSecretId`
  with a `toast.error`, mirroring
  `SecretManagerConnectionsDialog.tsx:64-77`'s existing pre-submit checks.
- Reset all local state (`editing`, any fetched-secrets cache) when `open`
  becomes `false`, mirroring
  `SecretManagerConnectionsDialog.tsx:52-58`'s existing `useEffect`.

- [ ] **Step 2: Wire it into `TitleBar`**

In `src/components/title-bar/TitleBar.tsx`, add a second icon button next to
the existing Secret Manager Connections one, using the `Bot` icon from
`lucide-react`:

```typescript
import { Bot, Settings } from 'lucide-react';
// ...
const [showAgentConfigs, setShowAgentConfigs] = useState(false);
// ...
<Button
  variant='ghost'
  size='icon'
  className='h-7 w-7'
  aria-label='AI Agent configurations'
  onClick={() => setShowAgentConfigs(true)}
>
  <Bot className='h-4 w-4' aria-hidden='true' />
</Button>
// ...
<AgentConfigsDialog open={showAgentConfigs} onOpenChange={setShowAgentConfigs} />
```

placed alongside the existing `showSecretManagers`/`SecretManagerConnectionsDialog`
state and JSX exactly as shown in the current file
(`src/components/title-bar/TitleBar.tsx:11,30-38,42-45`).

- [ ] **Step 3: Verify the app builds and the dialog renders**

Run: `yarn tsc --noEmit`
Expected: succeeds — no type errors across the new component and its
`TitleBar` wiring.

Run: `yarn tauri dev`, open the app, click the new "AI Agent configurations"
icon in the title bar, and confirm the dialog opens showing "No agents
configured." (or the empty-vault-connections state, if no Secret Manager
Connection exists yet) — this is the manual verification this plan relies on
in place of a component test, per this task's introductory note.

- [ ] **Step 4: Commit**

```bash
git add src/components/settings/AgentConfigsDialog.tsx src/components/title-bar/TitleBar.tsx
git commit -m "feat(frontend): add AI Agents settings dialog"
```

---

## Next Plan

None — this is the last plan in subproject A. The overall ACP AI-assist
feature's next subproject is **B: ACP Transport (new `rocket-acp` crate
extended with process lifecycle, JSON-RPC-over-stdio framing, and chat-only
session turns)** — see the project memory `project_acp_ai_assist_feature.md`
for its scope. Subproject B needs its own brainstorming/spec/plan cycle; do
not start implementing it from this plan file alone.

## Post-Implementation Review

Before considering subproject A complete, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `src/lib/tauri-api.ts`, `src/lib/queries/agent-config-queries.ts`,
> `src/lib/queries/__tests__/agent-config-queries.test.tsx`,
> `src/components/settings/AgentConfigsDialog.tsx`,
> `src/components/title-bar/TitleBar.tsx`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — does the `AgentConfig`
>    TypeScript shape match `AgentConfigDto` field-for-field (Plan 04), and
>    do all four query hooks behave like their `SecretManagerConnection`
>    equivalents (list caching, invalidation on mutation)?
> 2. Code quality and UX versus this plan's Review Focus section (client-side
>    validation before IPC calls, empty-vault-connections state, distinct
>    test-failure messages, state reset on dialog close).
> 3. Frontend guardrail conformance per
>    `.claude/rules/frontend-component-guardrails.md` — shadcn/ui primitives
>    and `lucide-react` icons only, no raw form elements, no full top-level
>    store destructuring if this component reads any Zustand state.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `yarn vitest run src/lib/queries/__tests__/agent-config-queries.test.tsx`
> and `yarn tsc --noEmit`, and confirm they still pass. Report what you found
> and fixed.

Once this review comes back clean (or its fixes are applied and
re-verified), subproject A is complete. Update the project memory
`project_acp_ai_assist_feature.md` to mark subproject A's status as done
before starting subproject B's brainstorming.
