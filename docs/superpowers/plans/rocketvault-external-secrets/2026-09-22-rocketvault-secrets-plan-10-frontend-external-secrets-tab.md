# RocketVault Secrets Plan 10: Environment External Secrets Tab — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each environment a "External Secrets" tab next to its existing
"Variables" tab in `EnvironmentDialog`, so a user can bind a Secret Manager
connection + vault name to an alias, fetch the vault's secret *names* (never
values), and have those bindings saved as part of the environment through the
dialog's existing Save button/mutation — no separate save path.

**Architecture:** Three small, additive changes to
`src/components/environments/`: a tab switcher inside `EnvironmentDialog.tsx`
(Task 1), a new sibling component to `VariableTable.tsx` — `ExternalSecretsTab.tsx`
— that follows the exact same parent-child editing contract
(`bindings` + `onChange(idx, patch)` + `onAdd()` + `onRemove(idx)` +
`onSave()`/`isDirty`/`saveState`) rather than inventing a new one (Task 2), and
wiring that mirrors `updateVariable`/`addVariable`/`removeVariable` 1:1 for
`externalSecrets` (Task 3). No new UI framework, no raw form primitives — shadcn
`Tabs`/`Select`/`Badge`/`Input`/`Button` only, per this repo's hard rules.

**Tech Stack:** React, TypeScript, Zustand, TanStack Query, shadcn/ui,
lucide-react, Vitest, @testing-library/react, @testing-library/user-event.

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.5, "Frontend: External Secrets tab"). Plan index:
`docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md` (has the
full locked interface contract every plan in this series depends on).

**This is Plan 10 of 10 — the final plan in the series.** It depends on Plan 08
(Tauri commands: `list_secret_manager_connections`,
`fetch_external_secret_names`/equivalent — see the "Plan 09 dependency note"
below) and Plan 09 (frontend types + Secret Manager Connections UI).

---

## Findings from reading the actual source (do not re-derive — use these)

**`src/components/environments/EnvironmentDialog.tsx`** (current state, quoted
exactly):

- Selection state: `const [selectedName, setSelectedName] = useState<string | null>(...)`.
  The currently-edited environment is derived, not stored directly:
  `const selectedEnv = localEnvs.find((e) => e.name === selectedName) ?? null;`
  — **this is the state variable this plan scopes `ExternalSecretsTab` to.**
- Dirty/local-edit state: `const [isDirty, setIsDirty] = useState(false)` and
  `const [localEnvs, setLocalEnvs] = useState<Environment[]>(environments)`.
  `localEnvs` is synced from `useEnvironments(activeCollection)` in a `useEffect`
  that only overwrites `localEnvs` `if (!isDirty)` — so in-flight edits survive
  background refetches. A second effect resets `localEnvs`/`isDirty` on dialog
  re-open (`open && !wasOpen.current`).
- Editing pattern (`updateVariable`/`addVariable`/`removeVariable`, all
  `useCallback`): each calls `setLocalEnvs((prev) => prev.map((e) => e.name !==
  selectedEnv.name ? e : { ...e, variables: <new array> }))` then
  `setIsDirty(true)`. **This exact shape is what Task 3 replicates for
  `externalSecrets`.**
- Save: `saveSettings` (`useCallback`) calls
  `saveMutation.mutateAsync({ ...selectedEnv, variables: dedupeVariables(selectedEnv.variables) })`
  then `setIsDirty(false)`. `saveMutation` is `useSaveEnvironment(activeCollection)`
  from `@/lib/queries/environment-queries`. `saveSettings` is wrapped by
  `const { state: saveState, trigger: triggerSave } = useSaveButton(saveSettings, 'Failed to save changes')`.
  Because `saveSettings` spreads `...selectedEnv` wholesale, **`externalSecrets`
  is already included in every save once it exists on the `Environment` type —
  no change to `saveSettings` itself is needed.**
- `handleAddEnv` calls `saveMutation.mutateAsync({ name, variables: [] })` — this
  object literal will fail `yarn tsc --noEmit` once `Environment` requires
  `externalSecrets` (per the locked contract in `00-plan-index.md`, it's not
  optional). **Task 3 fixes this call site.**
- Render: right panel renders `<VariableTable variables={selectedEnv.variables}
  onChange={updateVariable} onAdd={addVariable} onRemove={removeVariable}
  onSave={() => void triggerSave()} isDirty={isDirty} saveState={saveState}
  variableContext={variableContext} />` directly — **no existing tab switcher
  of any kind.** This is the insertion point for Task 1.
- No test file exists yet (`find` for `EnvironmentDialog.test.tsx` returned
  nothing) — Task 1/3 tests create
  `src/components/environments/EnvironmentDialog.test.tsx` from scratch.

**`src/components/environments/VariableTable.tsx`** (exact prop shape, quoted):

```typescript
interface VariableTableProps {
  variables: Variable[];
  onChange: (idx: number, patch: Partial<Variable>) => void;
  onAdd: () => void;
  onRemove: (idx: number) => void;
  onSave: () => void;
  isDirty: boolean;
  saveState: SaveButtonState;
  variableContext?: Map<string, VariableScopeEntry>;
}
```

This is a **per-index patch pattern**, not a single "hand back the whole
array" callback. The spec's §4.5 prose and this plan's own task prompt
describe `ExternalSecretsTab`'s `onChange` more loosely as
`onChange(bindings: ExternalSecretBinding[])`; per this plan's explicit
instruction to match `VariableTable`'s *actual* pattern rather than invent a
new shape, **Task 2 uses the per-index pattern instead** (`bindings` +
`onChange(idx, patch: Partial<ExternalSecretBinding>)` + `onAdd()` +
`onRemove(idx)`), which is what Task 3's wiring in `EnvironmentDialog.tsx`
naturally produces by mirroring `updateVariable`/`addVariable`/`removeVariable`.
The "replace `secretNames` wholesale" requirement from spec §4.4 is satisfied
inside `ExternalSecretsTab` itself: a successful "Fetch Secrets" call issues
one `onChange(idx, { secretNames: freshList })` patch, which replaces (not
merges) that one field on that one binding — the wholesale-replacement
semantics are about the `secretNames` array within a binding, not about
`ExternalSecretsTab`'s callback arity.

Delete-row icon: `VariableTable` imports `X` from `lucide-react` (see its
import line: `import { Check, Eye, EyeOff, Loader2, Plus, Save, X } from
'lucide-react';`) and renders it as
`<X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />`
inside a ghost icon `Button` with `aria-label={`Delete variable ${idx + 1}`}`.
**`ExternalSecretsTab` reuses `X` for its own per-row delete button**, same
styling, `aria-label={`Delete binding ${idx + 1}`}`.

`VariableTable`'s footer row combines the "Add Variable" button (left,
`Plus` icon, ghost) and the Save button (right, `Save`/`Loader2`/`Check` icon
depending on `saveState`) in one flex row. **`ExternalSecretsTab` mirrors this
exact footer** — see "Where does Save live when External Secrets is active?"
below.

**`.claude/rules/frontend-component-guardrails.md`** states: "Single-line
variable-aware fields: SingleLineEditor. Multi-line editor surfaces: Monaco."
— the operative word is *variable-aware*. Checking how the codebase actually
treats a plain, non-`{{var}}` single-line field:
`src/components/environments/InlineEnvName.tsx` (the environment's own `name`
field, edited inline in the sidebar) uses a plain shadcn
`<Input autoFocus className='h-7 text-sm flex-1 min-w-0' value={draftName}
onChange={(e) => setDraftName(e.target.value)} .../>` — **not**
`SingleLineEditor**. `VariableTable`'s **key** field (also a plain identifier,
no `{{var}}` interpolation) likewise uses plain `Input`; only its **value**
field uses `SingleLineEditor` (because values resolve `{{var}}` references).
The spec's own §4.3 description of the Secret Manager connection dialog's base
URL field makes the same distinction explicit: `SingleLineEditor`, "no
`{{var}}` awareness needed here since this isn't request-scoped" — implying
fields that genuinely don't need variable awareness should just be `Input`.
**Decision: `alias` and `vault name` in `ExternalSecretsTab` are plain
identifiers/literals with no `{{var}}` interpolation semantics (an alias is a
namespace key, a vault name is a literal RocketVault vault identifier passed
directly to `list_secrets`/`fetch_external_secret_names`, per spec §4.4) — both
use plain shadcn `Input`, matching `InlineEnvName`'s and `VariableTable`'s key
field's precedent, not `SingleLineEditor`.**

**`src/components/panes/EditorGroup.tsx`** uses a string-union `activeSection`
prop threaded from tab state (`activeTab.activeSection === 'overview' ? (...) :
activeTab.activeSection === 'environments' ? (...) : ...`) to switch between
entirely different top-level pane views tied to persisted per-editor-tab state.
That pattern exists because `activeSection` is state that belongs to the
*editor tab* (persisted, one per open tab) — it is not local dialog UI state.
`EnvironmentDialog`'s Variables/External Secrets switch has no such
persistence requirement; it is exactly the same shape of problem
`src/components/request/ScriptsTab.tsx` already solves with shadcn `Tabs`
(`<Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as ScriptPhase)}>`
with `TabsList`/`TabsTrigger`/`TabsContent`, imported from
`@/components/ui/tabs`). **Decision: use shadcn `Tabs`, matching
`ScriptsTab.tsx`'s precedent, not `EditorGroup.tsx`'s `activeSection`
string-union prop-drilling pattern** — `Tabs` is a local, unpersisted UI
concern here, and the codebase already has a working shadcn-`Tabs`-based
example of exactly this shape one component away in the same `request/`
family.

**Where does Save live when External Secrets is active?** `VariableTable`
owns and renders its own Save button inside its own footer; it is not lifted
out to `EnvironmentDialog`. If `Tabs` simply swaps `VariableTable` out for
`ExternalSecretsTab`, the Save button disappears while External Secrets is
active unless `ExternalSecretsTab` renders one too. Rather than restructure
`VariableTable` (out of this plan's scope — Task 1 says "Existing
`VariableTable` renders under Variables" unchanged) or duplicate a Save button
above the `Tabs` (visually redundant with `VariableTable`'s own footer button
whenever Variables is active), **`ExternalSecretsTab` takes the same
`onSave`/`isDirty`/`saveState` props as `VariableTable` and renders an
equivalent footer row** (Add Binding button, left; Save button, right — same
icon/label states as `VariableTable`'s). Both tabs' Save buttons call the
*same* `() => void triggerSave()` closure and read the *same* `isDirty`/
`saveState` from `EnvironmentDialog` — one `useSaveButton` instance, one
mutation, satisfies "no separate save path" from Task 3's brief even though
the button is rendered by two different components depending on which tab is
active.

## Plan 09 dependency note (read before Task 2)

At the time this plan was written, `2026-09-22-rocketvault-secrets-plan-09-frontend-connections.md`
did not yet exist in `docs/superpowers/plans/rocketvault-external-secrets/`
(only `00`, `01`, `02`, `03`, `05`, `07` were present — `09` was presumably
being authored in parallel). This plan cannot read Plan 09's actual function/
hook names, so Task 2 below uses the exact type names from `00-plan-index.md`'s
locked interface contract (`ExternalSecretBinding`, `ExternalSecretRef`,
`SecretManagerConnection`) plus the following **assumed** function names,
chosen to match this codebase's existing naming conventions
(`listEnvironments`/`useEnvironments` in `src/lib/tauri-api.ts` /
`src/lib/queries/environment-queries.ts`):

- `listSecretManagerConnections(): Promise<SecretManagerConnection[]>` —
  assumed to live in `src/lib/tauri-api.ts`, wrapping the
  `list_secret_manager_connections` Tauri command from spec §4.3.
- `useSecretManagerConnections()` — assumed React Query hook, likely in a new
  `src/lib/queries/secret-manager-queries.ts`, following the exact
  `useEnvironments` pattern (`useQuery({ queryKey: [...], queryFn:
  listSecretManagerConnections })`).
- `fetchExternalSecretNames(connectionId: string, vaultName: string):
  Promise<ExternalSecretRef[]>` — assumed to live in `src/lib/tauri-api.ts`,
  wrapping whatever Tauri command Plan 08 exposes for the "Fetch Secrets"
  action (the plan index's §4.3 names `test_secret_manager_connection`
  explicitly but does not name a names-fetch command — Plan 08's actual
  command name is authoritative).

**Before starting Task 2, the implementer MUST open the finalized
`2026-09-22-rocketvault-secrets-plan-09-frontend-connections.md` (it should
exist by the time Plan 08/09 are done, per the dependency order in
`00-plan-index.md`) and reconcile these three names against whatever it
actually landed with.** If they differ, use Plan 09's real names and update
the import lines below accordingly — do not silently implement against
possibly-stale assumed names without checking first.

---

## Global Constraints

- No raw `<button>`/`<input>`/`<select>` — shadcn `Tabs`, `Select`, `Badge`,
  `Input`, `Button` only, per `.claude/rules/frontend-component-guardrails.md`.
- Icons: `lucide-react` only, no inline SVGs.
- `alias` and `vault name` fields use plain shadcn `Input`, **not**
  `SingleLineEditor` — see the guardrails finding above (neither field is
  `{{var}}`-aware).
- Zustand: `EnvironmentDialog` already uses narrow selectors off `useEnvStore`
  (`useEnvStore((s) => s.activeCollection)`, etc.) — do not introduce a full
  destructure when adding any new store reads in this plan (none are expected;
  all new state is local `useState`, matching `isDirty`/`localEnvs`).
- Test code in this plan follows the codebase's existing Vitest conventions:
  `@testing-library/react` + `@testing-library/user-event`, `vi.mock('@/lib/tauri-api', ...)`
  with `vi.importActual` to preserve untouched exports (see
  `src/components/git/__tests__/GitCredentialsDialog.test.tsx` and
  `src/components/collections/__tests__/CollectionNode.test.tsx` for the exact
  pattern), and `QueryClient`/`QueryClientProvider` wrapping
  (`new QueryClient({ defaultOptions: { queries: { retry: false } } })`) for
  any component under test that uses a React Query hook.
- `useEnvStore` is a plain Zustand store (not a React context provider) — tests
  set fixture state directly via `useEnvStore.setState({...})` before
  rendering, no `<Provider>` wrapper needed for it specifically.
- No secret *values* ever appear in `ExternalSecretsTab`'s UI or test fixtures
  — only `ExternalSecretRef { name, secretId }` pairs, per spec §4.4/§4.8.

---

## Task 1: Tab switcher in `EnvironmentDialog.tsx`

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx`
- Create: `src/components/environments/EnvironmentDialog.test.tsx`

**Interfaces:**
- Consumes: nothing new (uses existing `selectedEnv`/`VariableTable`).
- Produces: an `activeDialogTab` local state value (`'variables' |
  'external-secrets'`, default `'variables'`) that Task 3 reuses to decide
  which tab's content renders `ExternalSecretsTab`.

- [ ] **Step 1: Write the failing test**

Create `src/components/environments/EnvironmentDialog.test.tsx`:

```typescript
// src/components/environments/EnvironmentDialog.test.tsx

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';
import type { Environment } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listEnvironments: vi.fn(),
    saveEnvironment: vi.fn(),
    deleteEnvironment: vi.fn(),
    getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
    getGlobalEnvironment: vi.fn().mockResolvedValue(null),
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
  };
});

const prodEnv: Environment = {
  name: 'prod',
  variables: [{ key: 'HOST', value: 'https://api.example.com', enabled: true, secret: false }],
  externalSecrets: [],
};

function renderDialog() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  useEnvStore.setState({ activeCollection: 'my-collection', activeEnvId: null });
  return render(
    <QueryClientProvider client={queryClient}>
      <EnvironmentDialog open onOpenChange={vi.fn()} />
    </QueryClientProvider>,
  );
}

describe('EnvironmentDialog tab switcher', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
  });

  it('defaults to the Variables tab and shows the variable table', async () => {
    renderDialog();
    expect(await screen.findByLabelText('Variable key 1')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /fetch secrets/i })).not.toBeInTheDocument();
  });

  it('switches to External Secrets and hides the variable table', async () => {
    renderDialog();
    await screen.findByLabelText('Variable key 1');
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: /external secrets/i }));

    expect(screen.queryByLabelText('Variable key 1')).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /add binding/i })).toBeInTheDocument();
  });

  it('switches back to Variables and shows the variable table again', async () => {
    renderDialog();
    await screen.findByLabelText('Variable key 1');
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: /external secrets/i }));
    await screen.findByRole('button', { name: /add binding/i });
    await user.click(screen.getByRole('tab', { name: /^variables$/i }));

    expect(await screen.findByLabelText('Variable key 1')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test EnvironmentDialog`
Expected: FAIL — no `tab` role named "External Secrets" exists yet, and
`ExternalSecretsTab`/"Add Binding" do not exist yet.

- [ ] **Step 3: Add the tab switcher**

In `src/components/environments/EnvironmentDialog.tsx`:

Add imports:

```typescript
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ExternalSecretsTab } from './ExternalSecretsTab';
```

Add local state alongside the existing `selectedName`/`isDirty` state:

```typescript
const [activeDialogTab, setActiveDialogTab] = useState<'variables' | 'external-secrets'>(
  'variables',
);
```

Reset it alongside `isDirty` in the existing "reset when switching environments"
effect (find `useEffect(() => { setIsDirty(false); }, [selectedName]);` and add
the reset there — switching environments should not silently strand the user on
a tab scoped to the *previous* environment's data):

```typescript
useEffect(() => {
  setIsDirty(false);
  setActiveDialogTab('variables');
}, [selectedName]);
```

Replace the right-panel `<VariableTable ... />` block with:

```tsx
{selectedEnv ? (
  <Tabs
    value={activeDialogTab}
    onValueChange={(v) => setActiveDialogTab(v as 'variables' | 'external-secrets')}
    className='flex-1 flex flex-col min-h-0'
  >
    <TabsList className='shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2'>
      <TabsTrigger value='variables' className='text-xs'>
        Variables
      </TabsTrigger>
      <TabsTrigger value='external-secrets' className='text-xs'>
        External Secrets
      </TabsTrigger>
    </TabsList>
    <TabsContent value='variables' className='flex-1 flex flex-col min-h-0 m-0'>
      <VariableTable
        variables={selectedEnv.variables}
        onChange={updateVariable}
        onAdd={addVariable}
        onRemove={removeVariable}
        onSave={() => void triggerSave()}
        isDirty={isDirty}
        saveState={saveState}
        variableContext={variableContext}
      />
    </TabsContent>
    <TabsContent value='external-secrets' className='flex-1 flex flex-col min-h-0 m-0'>
      <ExternalSecretsTab
        bindings={selectedEnv.externalSecrets}
        onChange={updateExternalSecret}
        onAdd={addExternalSecret}
        onRemove={removeExternalSecret}
        onSave={() => void triggerSave()}
        isDirty={isDirty}
        saveState={saveState}
      />
    </TabsContent>
  </Tabs>
) : (
  <div className='flex-1 flex flex-col items-center justify-center gap-4 text-center px-6 bg-gradient-to-b from-background to-card/60'>
    <RocketIdle className='w-24 h-24 opacity-70' />
    <div className='space-y-1'>
      <p className='text-sm font-medium text-foreground'>No environment selected</p>
      <p className='text-xs text-muted-foreground leading-relaxed'>
        Pick one from the list or create a new environment.
      </p>
    </div>
  </div>
)}
```

`updateExternalSecret`/`addExternalSecret`/`removeExternalSecret` do not exist
yet — Task 3 adds them. This task's test suite (Step 1) only exercises tab
switching and `VariableTable` visibility, so it does not require those
handlers to exist for its own assertions to pass, but the file will not
compile without at least stub versions. Add minimal stubs now (Task 3 fleshes
them out) so `yarn tsc --noEmit` and this task's tests pass in isolation:

```typescript
const updateExternalSecret = useCallback(() => {}, []);
const addExternalSecret = useCallback(() => {}, []);
const removeExternalSecret = useCallback(() => {}, []);
```

(These stubs are placeholders only for this task's compile step — Task 3
replaces them with the real `setLocalEnvs`/`setIsDirty` implementations before
this plan's Milestone Checklist is considered done. Do not leave them as
no-ops past Task 3.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test EnvironmentDialog`
Expected: PASS — 3 tests (default-to-Variables, switch-to-External-Secrets,
switch-back-to-Variables). Note this requires `ExternalSecretsTab.tsx` to
exist (even as Task 3's stub-wired version) — if running Task 1 and Task 2 out
of order, do Task 2 first or stub `ExternalSecretsTab` minimally to unblock
this step, then flesh it out in Task 2 proper.

- [ ] **Step 5: Commit**

Before committing, invoke the `dev-workflow-skills:1-git-commit` skill (per
this repo's commit convention) rather than a freeform `git commit -m`. Stage
`src/components/environments/EnvironmentDialog.tsx` and
`src/components/environments/EnvironmentDialog.test.tsx`.

---

## Task 2: `ExternalSecretsTab.tsx`

**Files:**
- Create: `src/components/environments/ExternalSecretsTab.tsx`
- Create: `src/components/environments/ExternalSecretsTab.test.tsx`

**Interfaces:**
- Consumes: `ExternalSecretBinding`, `ExternalSecretRef`, `SecretManagerConnection`
  types (locked contract, `00-plan-index.md`); `listSecretManagerConnections`/
  `useSecretManagerConnections`/`fetchExternalSecretNames` from Plan 09 (see the
  "Plan 09 dependency note" above — verify names before implementing).
- Produces: `ExternalSecretsTab` component, consumed by Task 1's
  `EnvironmentDialog.tsx` render and Task 3's wiring.

- [ ] **Step 1: Write the failing tests**

Create `src/components/environments/ExternalSecretsTab.test.tsx`:

```typescript
// src/components/environments/ExternalSecretsTab.test.tsx

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ExternalSecretsTab } from '@/components/environments/ExternalSecretsTab';
import type { ExternalSecretBinding, SecretManagerConnection } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listSecretManagerConnections: vi.fn(),
    fetchExternalSecretNames: vi.fn(),
  };
});

const connection: SecretManagerConnection = {
  id: 'conn-1',
  label: 'Prod RocketVault',
  baseUrl: 'https://vault.internal:8774',
  clientId: 'rocketapi',
  verifySsl: true,
  allowInsecureHttp: false,
};

const binding: ExternalSecretBinding = {
  alias: 'payments',
  connectionId: 'conn-1',
  vaultName: 'prod-vault',
  secretNames: [],
};

function renderTab(bindings: ExternalSecretBinding[], overrides: Partial<{
  onChange: (idx: number, patch: Partial<ExternalSecretBinding>) => void;
  onAdd: () => void;
  onRemove: (idx: number) => void;
}> = {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onChange = overrides.onChange ?? vi.fn();
  const onAdd = overrides.onAdd ?? vi.fn();
  const onRemove = overrides.onRemove ?? vi.fn();
  render(
    <QueryClientProvider client={queryClient}>
      <ExternalSecretsTab
        bindings={bindings}
        onChange={onChange}
        onAdd={onAdd}
        onRemove={onRemove}
        onSave={vi.fn()}
        isDirty={false}
        saveState='idle'
      />
    </QueryClientProvider>,
  );
  return { onChange, onAdd, onRemove };
}

describe('ExternalSecretsTab', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listSecretManagerConnections).mockResolvedValue([connection]);
  });

  it('fetches secret names and replaces secretNames wholesale via onChange', async () => {
    vi.mocked(tauriApi.fetchExternalSecretNames).mockResolvedValue([
      { name: 'stripe-key', secretId: 'b6f1c2e0-0000-0000-0000-000000000001' },
      { name: 'webhook-secret', secretId: 'b6f1c2e0-0000-0000-0000-000000000002' },
    ]);
    const { onChange } = renderTab([binding]);
    const user = userEvent.setup();

    await screen.findByText('Prod RocketVault');
    await user.click(screen.getByRole('button', { name: /fetch secrets/i }));

    await screen.findByText('stripe-key');
    expect(screen.getByText('webhook-secret')).toBeInTheDocument();
    expect(onChange).toHaveBeenCalledWith(0, {
      secretNames: [
        { name: 'stripe-key', secretId: 'b6f1c2e0-0000-0000-0000-000000000001' },
        { name: 'webhook-secret', secretId: 'b6f1c2e0-0000-0000-0000-000000000002' },
      ],
    });
  });

  it('replaces (not merges) an existing secretNames list on a second fetch', async () => {
    const populated: ExternalSecretBinding = {
      ...binding,
      secretNames: [{ name: 'old-name', secretId: 'old-id' }],
    };
    vi.mocked(tauriApi.fetchExternalSecretNames).mockResolvedValue([
      { name: 'new-name', secretId: 'new-id' },
    ]);
    const { onChange } = renderTab([populated]);
    const user = userEvent.setup();

    await screen.findByText('old-name');
    await user.click(screen.getByRole('button', { name: /fetch secrets/i }));

    await screen.findByText('new-name');
    expect(screen.queryByText('old-name')).not.toBeInTheDocument();
    expect(onChange).toHaveBeenLastCalledWith(0, {
      secretNames: [{ name: 'new-name', secretId: 'new-id' }],
    });
  });

  it('appends an empty binding on Add Binding', async () => {
    const { onAdd } = renderTab([]);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /add binding/i }));

    expect(onAdd).toHaveBeenCalledTimes(1);
  });

  it('removes a binding row via its delete button', async () => {
    const { onRemove } = renderTab([binding]);
    const user = userEvent.setup();

    await screen.findByText('Prod RocketVault');
    await user.click(screen.getByRole('button', { name: /delete binding 1/i }));

    expect(onRemove).toHaveBeenCalledWith(0);
  });

  it('calls onChange with a patch when the alias field is edited', async () => {
    const { onChange } = renderTab([binding]);
    const user = userEvent.setup();

    const aliasInput = await screen.findByLabelText('Alias for binding 1');
    await user.clear(aliasInput);
    await user.type(aliasInput, 'p');

    expect(onChange).toHaveBeenCalledWith(0, { alias: 'p' });
  });

  it('calls onChange with a patch when the vault name field is edited', async () => {
    const { onChange } = renderTab([binding]);
    const user = userEvent.setup();

    const vaultInput = await screen.findByLabelText('Vault name for binding 1');
    await user.clear(vaultInput);
    await user.type(vaultInput, 'v');

    expect(onChange).toHaveBeenCalledWith(0, { vaultName: 'v' });
  });

  it('renders fetched secret names as read-only badge chips', async () => {
    const populated: ExternalSecretBinding = {
      ...binding,
      secretNames: [{ name: 'stripe-key', secretId: 'id-1' }],
    };
    renderTab([populated]);

    const badge = await screen.findByText('stripe-key');
    expect(badge.closest('[data-slot="badge"]') ?? badge).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test ExternalSecretsTab`
Expected: FAIL — `Cannot find module '@/components/environments/ExternalSecretsTab'`.

- [ ] **Step 3: Implement `ExternalSecretsTab.tsx`**

```tsx
// src/components/environments/ExternalSecretsTab.tsx

import { Check, Download, Loader2, Plus, Save, X } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { fetchExternalSecretNames } from '@/lib/tauri-api';
import type { ExternalSecretBinding } from '@/lib/tauri-api';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import { cn } from '@/lib/utils';

const GRID_COLS = 'grid-cols-[1fr_1fr_1fr_auto_28px]';

function emptyBinding(): ExternalSecretBinding {
  return { alias: '', connectionId: '', vaultName: '', secretNames: [] };
}

interface ExternalSecretsTabProps {
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ExternalSecretBinding>) => void;
  onAdd: () => void;
  onRemove: (idx: number) => void;
  onSave: () => void;
  isDirty: boolean;
  saveState: SaveButtonState;
}

export function ExternalSecretsTab({
  bindings,
  onChange,
  onAdd,
  onRemove,
  onSave,
  isDirty,
  saveState,
}: ExternalSecretsTabProps) {
  const { data: connections = [] } = useSecretManagerConnections();
  const [fetchingIdx, setFetchingIdx] = useState<number | null>(null);

  const fetchSecrets = async (idx: number, binding: ExternalSecretBinding) => {
    if (!binding.connectionId || !binding.vaultName) return;
    setFetchingIdx(idx);
    try {
      // Wholesale replace, per spec 4.4 — never merged with the prior list.
      const secretNames = await fetchExternalSecretNames(binding.connectionId, binding.vaultName);
      onChange(idx, { secretNames });
    } catch (err) {
      console.error('[ExternalSecretsTab] fetch secrets failed:', err);
      toast.error('Failed to fetch secrets');
    } finally {
      setFetchingIdx(null);
    }
  };

  return (
    <div className='flex-1 flex flex-col min-w-0'>
      <div
        className={cn(
          'grid min-w-0 items-center gap-1.5 px-3 pt-3 pb-1.5 border-b border-border/40 shrink-0',
          GRID_COLS,
        )}
      >
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Alias
        </p>
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Connection
        </p>
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Vault Name
        </p>
        <div />
        <div />
      </div>

      {bindings.length === 0 ? (
        <div className='flex-1 flex flex-col items-center justify-center gap-3 text-center px-6'>
          <p className='text-sm font-medium text-foreground'>No external secrets bound</p>
          <p className='text-xs text-muted-foreground leading-relaxed max-w-[260px]'>
            Bind a RocketVault connection and vault to fetch secret names for
            this environment.
          </p>
          <Button variant='outline' size='sm' onClick={onAdd} className='gap-1.5'>
            <Plus className='h-3.5 w-3.5' />
            Add Binding
          </Button>
        </div>
      ) : (
        <ScrollArea className='flex-1'>
          <div className='px-3 pt-2 pb-1 space-y-2'>
            {bindings.map((binding, idx) => {
              const isFetching = fetchingIdx === idx;
              const canFetch = !!binding.connectionId && !!binding.vaultName && !isFetching;
              return (
                // biome-ignore lint/suspicious/noArrayIndexKey: index is stable here — rows are not reordered
                <div key={idx} className='space-y-1.5 pb-2 border-b border-border/20 last:border-0'>
                  <div className={cn('grid min-w-0 items-center gap-1.5 group', GRID_COLS)}>
                    <Input
                      placeholder='Alias'
                      value={binding.alias}
                      onChange={(e) => onChange(idx, { alias: e.target.value })}
                      className='h-7 min-w-0 text-xs font-mono'
                      aria-label={`Alias for binding ${idx + 1}`}
                    />
                    <Select
                      value={binding.connectionId}
                      onValueChange={(v) => onChange(idx, { connectionId: v })}
                    >
                      <SelectTrigger
                        className='h-7 min-w-0 text-xs'
                        aria-label={`Connection for binding ${idx + 1}`}
                      >
                        <SelectValue placeholder='Select connection' />
                      </SelectTrigger>
                      <SelectContent>
                        {connections.map((conn) => (
                          <SelectItem key={conn.id} value={conn.id} className='text-xs'>
                            {conn.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <Input
                      placeholder='Vault name'
                      value={binding.vaultName}
                      onChange={(e) => onChange(idx, { vaultName: e.target.value })}
                      className='h-7 min-w-0 text-xs font-mono'
                      aria-label={`Vault name for binding ${idx + 1}`}
                    />
                    <Button
                      variant='outline'
                      size='sm'
                      className='h-7 text-xs gap-1.5 shrink-0'
                      disabled={!canFetch}
                      onClick={() => void fetchSecrets(idx, binding)}
                    >
                      {isFetching ? (
                        <Loader2 className='h-3.5 w-3.5 animate-spin' />
                      ) : (
                        <Download className='h-3.5 w-3.5' />
                      )}
                      Fetch Secrets
                    </Button>
                    <TooltipProvider>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button
                            variant='ghost'
                            size='icon'
                            className='h-6 w-6 shrink-0 opacity-60 group-hover:opacity-100 focus-visible:opacity-100 transition-opacity'
                            onClick={() => onRemove(idx)}
                            aria-label={`Delete binding ${idx + 1}`}
                          >
                            <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
                          </Button>
                        </TooltipTrigger>
                        <TooltipContent>Delete binding</TooltipContent>
                      </Tooltip>
                    </TooltipProvider>
                  </div>
                  <div className='pl-0.5 flex flex-wrap gap-1'>
                    {binding.secretNames.length === 0 ? (
                      <p className='text-[11px] text-muted-foreground/70'>
                        No secrets fetched yet.
                      </p>
                    ) : (
                      binding.secretNames.map((ref) => (
                        <Badge key={ref.secretId} variant='secondary' className='text-[11px] font-mono'>
                          {ref.name}
                        </Badge>
                      ))
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </ScrollArea>
      )}

      <div className='px-3 py-2 border-t border-border/40 flex items-center justify-between shrink-0'>
        <Button
          variant='ghost'
          size='sm'
          onClick={onAdd}
          className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
        >
          <Plus className='h-3.5 w-3.5' />
          Add Binding
        </Button>
        <Button
          size='sm'
          onClick={onSave}
          disabled={!isDirty || saveState !== 'idle'}
          className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
        >
          {saveState === 'saving' ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : saveState === 'success' ? (
            <Check className='h-3.5 w-3.5' />
          ) : (
            <Save className='h-3.5 w-3.5' />
          )}
          {saveState === 'success' ? 'Saved' : 'Save'}
        </Button>
      </div>
    </div>
  );
}
```

Note on the `Download` icon: `VariableTable` doesn't have a precedent for a
"fetch"-style action, so this plan picks `Download` (a plain, unambiguous
lucide-react icon for "pull data in") rather than inventing a new visual
language — swap it for `RefreshCw` if design review during implementation
prefers a "refresh" connotation instead; either is a `lucide-react` import, so
the guardrails rule is satisfied either way.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test ExternalSecretsTab`
Expected: PASS — 7 tests.

- [ ] **Step 5: `yarn tsc --noEmit` and `yarn check`**

Both must pass with zero errors before moving on — this file introduces new
imports (`useSecretManagerConnections`, `fetchExternalSecretNames`) that only
type-check once Plan 09 has actually landed those exports; if Plan 09 isn't
merged yet when this task is executed, this step will fail on missing exports
— that is expected and blocking, not a bug in this plan. Do not stub around it
with `any`; wait for Plan 09 or coordinate with whoever owns it.

- [ ] **Step 6: Commit**

Invoke `dev-workflow-skills:1-git-commit`. Stage
`src/components/environments/ExternalSecretsTab.tsx` and
`src/components/environments/ExternalSecretsTab.test.tsx`.

---

## Task 3: Fetch/save wiring end-to-end

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx`
- Modify: `src/components/environments/EnvironmentDialog.test.tsx`

**Interfaces:**
- Consumes: `ExternalSecretsTab` (Task 2), `ExternalSecretBinding` (locked
  contract).
- Produces: fully wired `externalSecrets` editing, persisted through the
  existing `saveMutation`/`useSaveEnvironment` — no new Tauri command call
  added in the frontend beyond what Plan 09/08 already expose.

- [ ] **Step 1: Write the failing test**

Extend `src/components/environments/EnvironmentDialog.test.tsx` (append to the
existing `describe` blocks, or add a new one in the same file per this
project's convention of one test file per component):

```typescript
describe('EnvironmentDialog external secrets save flow', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
    vi.mocked(tauriApi.saveEnvironment).mockResolvedValue(undefined);
  });

  it('saves a binding added via the External Secrets tab through the existing save mutation', async () => {
    renderDialog();
    const user = userEvent.setup();

    await screen.findByLabelText('Variable key 1');
    await user.click(screen.getByRole('tab', { name: /external secrets/i }));
    await user.click(await screen.findByRole('button', { name: /add binding/i }));

    const aliasInput = await screen.findByLabelText('Alias for binding 1');
    await user.type(aliasInput, 'payments');

    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, savedEnv] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(savedEnv.externalSecrets).toContainEqual(
      expect.objectContaining({ alias: 'payments', connectionId: '', vaultName: '' }),
    );
  });
});
```

(This mirrors the Variables-tab save flow shape implicitly exercised by Task
1's rendering tests — there was no pre-existing standalone "save" test for
`VariableTable`'s flow to copy verbatim, since `EnvironmentDialog.test.tsx`
did not exist before this plan; this test establishes that pattern for both
tabs going forward, asserting against `saveEnvironment` — the function
`useSaveEnvironment`'s mutation actually calls — rather than against the
mutation hook internals.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test EnvironmentDialog`
Expected: FAIL — the Task 1 stub handlers (`updateExternalSecret` etc.) are
no-ops, so `Add Binding` doesn't add a row and the alias input never appears.

- [ ] **Step 3: Replace the stub handlers with real wiring**

In `src/components/environments/EnvironmentDialog.tsx`, replace the Task 1
stubs with implementations that mirror `updateVariable`/`addVariable`/
`removeVariable` exactly, substituting `externalSecrets` for `variables`:

```typescript
const updateExternalSecret = useCallback(
  (idx: number, patch: Partial<ExternalSecretBinding>) => {
    if (!selectedEnv) return;
    setLocalEnvs((prev) =>
      prev.map((e) => {
        if (e.name !== selectedEnv.name) return e;
        const externalSecrets = e.externalSecrets.slice();
        externalSecrets[idx] = { ...externalSecrets[idx], ...patch };
        return { ...e, externalSecrets };
      }),
    );
    setIsDirty(true);
  },
  [selectedEnv],
);

const addExternalSecret = useCallback(() => {
  if (!selectedEnv) return;
  setLocalEnvs((prev) =>
    prev.map((e) => {
      if (e.name !== selectedEnv.name) return e;
      return {
        ...e,
        externalSecrets: [
          ...e.externalSecrets,
          { alias: '', connectionId: '', vaultName: '', secretNames: [] },
        ],
      };
    }),
  );
  setIsDirty(true);
}, [selectedEnv]);

const removeExternalSecret = useCallback(
  (idx: number) => {
    if (!selectedEnv) return;
    setLocalEnvs((prev) =>
      prev.map((e) => {
        if (e.name !== selectedEnv.name) return e;
        return { ...e, externalSecrets: e.externalSecrets.filter((_, i) => i !== idx) };
      }),
    );
    setIsDirty(true);
  },
  [selectedEnv],
);
```

Add `ExternalSecretBinding` to the existing `import type { Environment,
Variable } from '@/lib/tauri-api';` line.

Fix `handleAddEnv`'s save-mutation call, which currently omits the
now-required field:

```typescript
const handleAddEnv = useCallback(
  async (name: string) => {
    await saveMutation.mutateAsync({ name, variables: [], externalSecrets: [] });
    setSelectedName(name);
    setActiveEnvId(name);
  },
  [saveMutation, setActiveEnvId],
);
```

No change is needed to `saveSettings` itself — it already spreads
`...selectedEnv`, which now includes `externalSecrets`, into
`saveMutation.mutateAsync`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test EnvironmentDialog`
Expected: PASS — all tests from Task 1 and Task 3 in this file.

- [ ] **Step 5: Full verification for this task**

Run: `yarn tsc --noEmit`, `yarn check`, `yarn test`
Expected: all pass, including every other frontend test file (no regressions
from the `Environment`/`ExternalSecretBinding` type surface change).

- [ ] **Step 6: Commit**

Invoke `dev-workflow-skills:1-git-commit`. Stage
`src/components/environments/EnvironmentDialog.tsx` and
`src/components/environments/EnvironmentDialog.test.tsx`.

---

## Milestone Checklist — Plan 10

- [ ] `EnvironmentDialog.tsx` has a `Variables`/`External Secrets` tab switcher
      (shadcn `Tabs`), defaulting to `Variables`, reset to `Variables` whenever
      the selected environment changes
- [ ] `ExternalSecretsTab.tsx` renders alias (`Input`), connection picker
      (`Select`, sourced from `listSecretManagerConnections`/
      `useSecretManagerConnections`), vault name (`Input`), "Fetch Secrets"
      button, and read-only `Badge` chips for fetched names
- [ ] "Fetch Secrets" replaces a binding's `secretNames` wholesale, never
      merges — verified by a second-fetch test
- [ ] "Add Binding" appends an empty `ExternalSecretBinding`; delete button
      (`X`, matching `VariableTable`'s icon) removes a row
- [ ] `EnvironmentDialog`'s `updateExternalSecret`/`addExternalSecret`/
      `removeExternalSecret` mirror `updateVariable`/`addVariable`/
      `removeVariable` exactly, writing into the same `localEnvs`/`isDirty`
      state
- [ ] Saving (either tab's Save button) persists `externalSecrets` through the
      existing `useSaveEnvironment` mutation — no separate save path
- [ ] `handleAddEnv` updated to satisfy the now-required `externalSecrets`
      field on `Environment`
- [ ] All three tasks' Vitest suites pass: `EnvironmentDialog.test.tsx`,
      `ExternalSecretsTab.test.tsx`
- [ ] `yarn tsc --noEmit`, `yarn check`, `yarn test` all pass

## Next Plan

None — this is the final plan in the RocketVault External Secrets series.
Once this plan's Milestone Checklist is complete, run the full verification
suite: `cargo test --workspace`, `yarn tsc --noEmit`, `yarn check`, `yarn
test`, and do a manual end-to-end pass against a real local RocketVault
instance per spec acceptance criteria 1-6 (§8 of the spec), since CI does not
run against a live RocketVault server.
