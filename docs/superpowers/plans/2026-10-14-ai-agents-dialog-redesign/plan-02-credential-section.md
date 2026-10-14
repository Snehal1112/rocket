# AI Agents dialog redesign: plan 2, credential section

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the standalone credential part of the AI Agents form: the shared form types, a secret listing hook that auto-loads secrets with a stale-response guard, and `AgentCredentialSection` with the Secret manager, Environment and Keychain sources. The section is tested on its own. `AgentConfigsDialog.tsx` is not touched here, plan 3 wires it in.

**Architecture:** `form-types.ts` holds the credential slice of the form state. `use-secret-listing.ts` takes over the generation-guard logic from `handleFetchSecrets` in `AgentConfigsDialog.tsx` and adds a debounce for scope typing. `AgentCredentialSection.tsx` is a controlled component (`value` plus `onChange(patch)`) built from the plan 1 DSM components (`FormField`, `SegmentedControl`, `StateBlock`, `SecretField`). Provider availability comes from `src/lib/secret-providers.ts`, so there is no new IPC. The section fetches connections itself with `useSecretManagerConnections()`.

**Tech Stack:** React 19, TypeScript, TanStack Query, Radix Select and Tooltip through the shadcn wrappers, lucide-react, Vitest with Testing Library, Biome, Yarn.

**Spec:** `docs/superpowers/specs/2026-10-14-ai-agents-dialog-redesign-design.md` (sections Credential section and Help text). Index with the fixed contract: `docs/superpowers/plans/2026-10-14-ai-agents-dialog-redesign/00-plan-index.md`.

**Depends on plan 1.** These files must already exist: `src/components/ui/form-field.tsx`, `segmented-control.tsx`, `state-block.tsx`, `secret-field.tsx`. Use them through the signatures in the index. Do not redefine them.

---

## Global Constraints

- Hard rules from `CLAUDE.md`: shadcn primitives only (no raw `button`, `input`, `select`, `dialog`, `form`), `lucide-react` icons only (no inline SVGs), narrow Zustand selectors, and in production Rust no panicking unwrap calls. This plan has no Rust.
- Follow the Rocket DSM. Tokens only, no hard-coded colours.
- Write the NAME of a variable in capitals when it is a name and not a value (DSM rule).
- Persisted data is unchanged: `CredentialSource` stays `vault | env | keychain` and the `vault_*` fields stay.
- Every control comes from `src/components/ui/`. Help text is always visible and required (`FormField` `help` prop).
- Commits: conventional commit format, through the `dev-workflow-skills:1-git-commit` skill, with a pathspec (`git add <paths>` and `git commit --only -- <paths>`). Never `git add -A`, `--all` or `.`.
- Checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`.
- Code comments are short full sentences ending with a punctuation mark.
- Do not modify `src/components/settings/AgentConfigsDialog.tsx` in this plan.
- A repo hook blocks writes whose content contains the literal Rust panic call. Do not type that literal anywhere.

## Review Focus

Five inputs most likely to bite. Each is pinned by a test in the owning task.

1. **Connection deleted while editing.** A saved `vaultConnectionId` that is no longer in the list must warn, must not list secrets and must not crash. Task 2, test `warns and does not fetch when the saved connection was deleted`.
2. **Provider with no fetcher already saved on an old agent.** An AWS, HashiCorp or GCP connection saved earlier must show a clear warning and must not call the fetcher. Task 2, test `warns and does not fetch when the saved connection has no fetcher`.
3. **Rapid scope typing.** Typing in the scope field must not fire one fetch per keystroke, and the chosen secret must survive until the new list arrives. Task 1, test `debounces rapid scope typing into one fetch`. Task 2, test `keeps the chosen secret while the scope changes`.
4. **Empty secret list and a chosen secret missing from a new list.** Empty shows the empty state block. A missing chosen secret shows a warning and is never silently cleared. Task 1, test `becomes ready with an empty list`. Task 2, tests `shows an empty state when the vault has no secrets` and `warns when the chosen secret is missing from the list and keeps it`.
5. **Switching connection mid-load.** A slow response for the old connection must not overwrite the list of the new one, and a new connection must load with its own scope immediately, not after the debounce. Task 1, tests `ignores a stale response after the connection changes` and `loads at once when the connection changes`.

---

### Task 1: Form types and secret listing hook

**Files:**
- Create: `src/components/settings/agent-config/form-types.ts`
- Create: `src/components/settings/agent-config/use-secret-listing.ts`
- Create: `src/components/settings/agent-config/__tests__/form-types.test.ts`
- Create: `src/components/settings/agent-config/__tests__/use-secret-listing.test.ts`

**Interfaces:**

Consumes:

```ts
// src/lib/tauri-api.ts
export const fetchExternalSecretNames: (id: string, vaultName: string) => Promise<ExternalSecretRef[]>;
export interface ExternalSecretRef { name: string; secretId: string }
```

Produces (exact names from the index contract):

```ts
// form-types.ts
export type AgentKindValue = 'claude' | 'gemini' | 'generic';
export interface CredentialFormState {
  credentialSource: 'vault' | 'env' | 'keychain';
  credentialEnvVar: string;
  sourceEnvVar: string;
  localCredential: string;
  removeSavedCredential: boolean;
  vaultConnectionId: string;
  vaultName: string;
  vaultSecretId: string;
  vaultSecretName: string;
}
export type CredentialErrors = Partial<Record<keyof CredentialFormState, string | null>>;
export const EMPTY_CREDENTIAL_FORM: CredentialFormState; // extra helper for new agents and tests

// use-secret-listing.ts
export const SCOPE_DEBOUNCE_MS: number; // 400
export function useSecretListing(args: {
  connectionId: string;
  scope: string;
  enabled: boolean;
}): {
  status: 'idle' | 'loading' | 'ready' | 'error';
  secrets: { id: string; name: string }[];
  error: string | null;
  refresh: () => void;
};
```

Behaviour of the hook:

- `enabled` false or empty `connectionId`: status `idle`, no fetch, any in-flight response is dropped.
- Otherwise it fetches `fetchExternalSecretNames(connectionId, scope)`. Status goes `loading`, then `ready` or `error`. The list is empty while loading.
- A generation counter drops any response that arrives after a newer request, a dependency change or unmount. This is the guard moved from `handleFetchSecrets`.
- A scope change waits `SCOPE_DEBOUNCE_MS` before it fetches. A connection change fetches at once with the current scope.
- `refresh()` fetches again now. A pending debounced scope is applied first, so refresh never uses a stale scope.

- [ ] **Step 1: Write the failing form-types test**

Create `src/components/settings/agent-config/__tests__/form-types.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  type CredentialErrors,
  type CredentialFormState,
  EMPTY_CREDENTIAL_FORM,
} from '../form-types';

describe('form-types', () => {
  it('starts a new agent on the secret manager source with nothing filled in', () => {
    const empty: CredentialFormState = EMPTY_CREDENTIAL_FORM;
    expect(empty).toEqual({
      credentialSource: 'vault',
      credentialEnvVar: '',
      sourceEnvVar: '',
      localCredential: '',
      removeSavedCredential: false,
      vaultConnectionId: '',
      vaultName: '',
      vaultSecretId: '',
      vaultSecretName: '',
    });
  });

  it('allows an error message per credential field', () => {
    const errors: CredentialErrors = { credentialEnvVar: 'Bad name.', vaultName: null };
    expect(errors.credentialEnvVar).toBe('Bad name.');
    expect(errors.vaultName).toBeNull();
  });
});
```

- [ ] **Step 2: Write the failing hook test**

Create `src/components/settings/agent-config/__tests__/use-secret-listing.test.ts`:

```ts
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { SCOPE_DEBOUNCE_MS, useSecretListing } from '../use-secret-listing';

vi.mock('@/lib/tauri-api', () => ({ fetchExternalSecretNames: vi.fn() }));

type Refs = api.ExternalSecretRef[];

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const fetchMock = () => vi.mocked(api.fetchExternalSecretNames);

type Props = { connectionId: string; scope: string; enabled: boolean };
const start = (props: Props) => renderHook((p: Props) => useSecretListing(p), { initialProps: props });

describe('useSecretListing', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('stays idle and does not fetch while disabled', () => {
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: false });
    expect(result.current.status).toBe('idle');
    expect(result.current.secrets).toEqual([]);
    expect(fetchMock()).not.toHaveBeenCalled();
  });

  it('loads and maps the secrets', async () => {
    fetchMock().mockResolvedValue([{ name: 'anthropic', secretId: 's1' }]);
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    expect(result.current.status).toBe('loading');
    await vi.waitFor(() => expect(result.current.status).toBe('ready'));
    expect(fetchMock()).toHaveBeenCalledWith('c1', 'prod');
    expect(result.current.secrets).toEqual([{ id: 's1', name: 'anthropic' }]);
    expect(result.current.error).toBeNull();
  });

  it('becomes ready with an empty list', async () => {
    fetchMock().mockResolvedValue([] as Refs);
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    await vi.waitFor(() => expect(result.current.status).toBe('ready'));
    expect(result.current.secrets).toEqual([]);
  });

  it('reports an error and clears the list', async () => {
    fetchMock().mockRejectedValue('boom');
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    await vi.waitFor(() => expect(result.current.status).toBe('error'));
    expect(result.current.error).toBe('boom');
    expect(result.current.secrets).toEqual([]);
  });

  it('ignores a stale response after the connection changes', async () => {
    const a = deferred<Refs>();
    const b = deferred<Refs>();
    fetchMock().mockImplementation((id) => (id === 'a' ? a.promise : b.promise));
    const { result, rerender } = start({ connectionId: 'a', scope: 's', enabled: true });
    rerender({ connectionId: 'b', scope: 's', enabled: true });
    await act(async () => {
      b.resolve([{ name: 'B', secretId: 'b1' }]);
      await b.promise;
    });
    await act(async () => {
      a.resolve([{ name: 'A', secretId: 'a1' }]);
      await a.promise;
    });
    expect(result.current.status).toBe('ready');
    expect(result.current.secrets).toEqual([{ id: 'b1', name: 'B' }]);
  });

  it('drops a response that arrives after the hook was disabled', async () => {
    const a = deferred<Refs>();
    fetchMock().mockReturnValue(a.promise);
    const { result, rerender } = start({ connectionId: 'a', scope: 's', enabled: true });
    rerender({ connectionId: 'a', scope: 's', enabled: false });
    await act(async () => {
      a.resolve([{ name: 'A', secretId: 'a1' }]);
      await a.promise;
    });
    expect(result.current.status).toBe('idle');
    expect(result.current.secrets).toEqual([]);
  });

  it('debounces rapid scope typing into one fetch', async () => {
    vi.useFakeTimers();
    fetchMock().mockResolvedValue([]);
    const { rerender } = start({ connectionId: 'c1', scope: 'p', enabled: true });
    expect(fetchMock()).toHaveBeenCalledTimes(1);
    rerender({ connectionId: 'c1', scope: 'pr', enabled: true });
    rerender({ connectionId: 'c1', scope: 'pro', enabled: true });
    rerender({ connectionId: 'c1', scope: 'prod', enabled: true });
    expect(fetchMock()).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCOPE_DEBOUNCE_MS);
    });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
    expect(fetchMock()).toHaveBeenLastCalledWith('c1', 'prod');
  });

  it('loads at once when the connection changes', async () => {
    vi.useFakeTimers();
    fetchMock().mockResolvedValue([]);
    const { rerender } = start({ connectionId: 'a', scope: 'x', enabled: true });
    rerender({ connectionId: 'b', scope: 'y', enabled: true });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
    expect(fetchMock()).toHaveBeenLastCalledWith('b', 'y');
  });

  it('refreshes at once with the latest scope', async () => {
    vi.useFakeTimers();
    fetchMock().mockResolvedValue([]);
    const { result, rerender } = start({ connectionId: 'c1', scope: 'p', enabled: true });
    rerender({ connectionId: 'c1', scope: 'pr', enabled: true });
    act(() => {
      result.current.refresh();
    });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
    expect(fetchMock()).toHaveBeenLastCalledWith('c1', 'pr');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCOPE_DEBOUNCE_MS);
    });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
  });

  it('refreshes the current list when the scope did not change', async () => {
    fetchMock().mockResolvedValue([{ name: 'one', secretId: 's1' }]);
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    await vi.waitFor(() => expect(result.current.status).toBe('ready'));
    fetchMock().mockResolvedValue([
      { name: 'one', secretId: 's1' },
      { name: 'two', secretId: 's2' },
    ]);
    act(() => {
      result.current.refresh();
    });
    await vi.waitFor(() => expect(result.current.secrets).toHaveLength(2));
    expect(fetchMock()).toHaveBeenCalledTimes(2);
  });

  it('does nothing on refresh while disabled', () => {
    const { result } = start({ connectionId: '', scope: '', enabled: false });
    act(() => {
      result.current.refresh();
    });
    expect(fetchMock()).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run the tests and expect FAIL**

Run: `yarn test src/components/settings/agent-config/__tests__/form-types.test.ts src/components/settings/agent-config/__tests__/use-secret-listing.test.ts`

Expected: FAIL, both suites fail to resolve `../form-types` and `../use-secret-listing`.

- [ ] **Step 4: Implement `form-types.ts`**

Create `src/components/settings/agent-config/form-types.ts`:

```ts
// Which ACP agent a form edits. Matches AgentKind, where a missing value means claude.
export type AgentKindValue = 'claude' | 'gemini' | 'generic';

// The credential slice of the agent form. Persisted values are unchanged.
export interface CredentialFormState {
  credentialSource: 'vault' | 'env' | 'keychain';
  // UI label: "Agent reads it as". The variable the agent receives.
  credentialEnvVar: string;
  // UI label: "Rocket reads it from". The variable in Rocket's own environment.
  sourceEnvVar: string;
  localCredential: string;
  removeSavedCredential: boolean;
  vaultConnectionId: string;
  // For Azure this is saved as the connection label.
  vaultName: string;
  vaultSecretId: string;
  vaultSecretName: string;
}

export type CredentialErrors = Partial<Record<keyof CredentialFormState, string | null>>;

// The credential state of a brand new agent.
export const EMPTY_CREDENTIAL_FORM: CredentialFormState = {
  credentialSource: 'vault',
  credentialEnvVar: '',
  sourceEnvVar: '',
  localCredential: '',
  removeSavedCredential: false,
  vaultConnectionId: '',
  vaultName: '',
  vaultSecretId: '',
  vaultSecretName: '',
};
```

- [ ] **Step 5: Implement `use-secret-listing.ts`**

Create `src/components/settings/agent-config/use-secret-listing.ts`:

```ts
import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchExternalSecretNames } from '@/lib/tauri-api';

// How long scope typing must pause before the secrets are fetched again.
export const SCOPE_DEBOUNCE_MS = 400;

type ListingStatus = 'idle' | 'loading' | 'ready' | 'error';

interface ListingState {
  status: ListingStatus;
  secrets: { id: string; name: string }[];
  error: string | null;
}

const IDLE: ListingState = { status: 'idle', secrets: [], error: null };

interface SettledScope {
  connectionId: string;
  scope: string;
}

// Lists the secrets of a connection and scope, and keeps the list fresh.
export function useSecretListing(args: { connectionId: string; scope: string; enabled: boolean }): {
  status: ListingStatus;
  secrets: { id: string; name: string }[];
  error: string | null;
  refresh: () => void;
} {
  const { connectionId, scope, enabled } = args;
  const [state, setState] = useState<ListingState>(IDLE);
  const [settled, setSettled] = useState<SettledScope>({ connectionId, scope });
  // Bumped whenever the list becomes stale. A fetch that resolves after a bump is dropped.
  const generation = useRef(0);

  // A new connection uses its own scope at once. Only typing in the scope is debounced.
  const settledScope = settled.connectionId === connectionId ? settled.scope : scope;

  useEffect(() => {
    if (settled.connectionId !== connectionId) {
      setSettled({ connectionId, scope });
      return;
    }
    if (settled.scope === scope) return;
    const timer = setTimeout(() => setSettled({ connectionId, scope }), SCOPE_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [connectionId, scope, settled]);

  const load = useCallback((id: string, listScope: string) => {
    const current = ++generation.current;
    setState({ status: 'loading', secrets: [], error: null });
    fetchExternalSecretNames(id, listScope).then(
      (refs) => {
        if (current !== generation.current) return;
        setState({
          status: 'ready',
          secrets: refs.map((ref) => ({ id: ref.secretId, name: ref.name })),
          error: null,
        });
      },
      (e: unknown) => {
        if (current !== generation.current) return;
        setState({ status: 'error', secrets: [], error: String(e) });
      },
    );
  }, []);

  useEffect(() => {
    if (!enabled || connectionId === '') {
      generation.current += 1;
      setState(IDLE);
      return;
    }
    load(connectionId, settledScope);
    return () => {
      // Drops the response of this run after a dependency change or unmount.
      generation.current += 1;
    };
  }, [enabled, connectionId, settledScope, load]);

  const refresh = useCallback(() => {
    if (!enabled || connectionId === '') return;
    if (scope === settledScope) {
      load(connectionId, scope);
    } else {
      // The effect above fetches once the settled scope catches up.
      setSettled({ connectionId, scope });
    }
  }, [enabled, connectionId, scope, settledScope, load]);

  return { status: state.status, secrets: state.secrets, error: state.error, refresh };
}
```

- [ ] **Step 6: Run the tests and expect PASS**

Run: `yarn test src/components/settings/agent-config/__tests__/form-types.test.ts src/components/settings/agent-config/__tests__/use-secret-listing.test.ts`

Expected: PASS, 2 plus 11 tests.

- [ ] **Step 7: Type check and lint**

Run: `yarn tsc --noEmit && yarn check`

Expected: no errors. If `yarn check` reports formatting only, run `yarn format` on the four files and re-run.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with this pathspec commit:

```bash
git add src/components/settings/agent-config/form-types.ts src/components/settings/agent-config/use-secret-listing.ts src/components/settings/agent-config/__tests__/form-types.test.ts src/components/settings/agent-config/__tests__/use-secret-listing.test.ts
git commit --only -m "feat(settings): add agent credential form types and secret listing hook" -- src/components/settings/agent-config/form-types.ts src/components/settings/agent-config/use-secret-listing.ts src/components/settings/agent-config/__tests__/form-types.test.ts src/components/settings/agent-config/__tests__/use-secret-listing.test.ts
```

---

### Task 2: Credential section, Secret manager source

**Files:**
- Create: `src/components/settings/agent-config/AgentCredentialSection.tsx`
- Create: `src/components/settings/agent-config/__tests__/section-harness.tsx`
- Create: `src/components/settings/agent-config/__tests__/AgentCredentialSection.vault.test.tsx`

This task ships the whole component file: the source picker, the Secret manager source, and also the Environment and Keychain field components and the shared "Agent reads it as" field, so the file compiles and is complete. Task 3 pins the Environment and Keychain behaviour with tests and fixes any gap.

**Interfaces:**

Consumes:

```ts
// plan 1
export function FormField(props: { id: string; label: ReactNode; optionalHint?: string; group?: boolean; help: ReactNode; error?: string | null; children: (a11y: FieldA11yProps) => ReactNode }): JSX.Element;
export interface FieldA11yProps { id: string; 'aria-invalid': boolean; 'aria-describedby': string | undefined }
export function SegmentedControl<T extends string>(props: { value: T; onValueChange: (value: T) => void; options: { value: T; label: string }[]; 'aria-label': string }): JSX.Element;
export function StateBlock(props: { tone: 'loading' | 'empty' | 'error'; children: ReactNode; action?: { label: string; onClick: () => void } }): JSX.Element;
export function SecretField(props: { id: string; value: string; onValueChange: (value: string) => void; hasSaved: boolean; markedForRemoval: boolean; onToggleRemove: () => void; placeholder?: string; 'aria-invalid'?: boolean; 'aria-describedby'?: string }): JSX.Element;
// this plan, task 1
export function useSecretListing(args: { connectionId: string; scope: string; enabled: boolean }): { status: 'idle' | 'loading' | 'ready' | 'error'; secrets: { id: string; name: string }[]; error: string | null; refresh: () => void };
// existing
export function useSecretManagerConnections(): UseQueryResult<SecretManagerConnection[]>;  // src/lib/queries/secret-manager-queries.ts
export function getProviderDescriptor(kind?: SecretProviderKind | null): SecretProviderDescriptor;  // src/lib/secret-providers.ts
```

Produces:

```ts
// AgentCredentialSection.tsx
export function AgentCredentialSection(props: {
  kind: AgentKindValue;
  value: CredentialFormState;
  onChange: (patch: Partial<CredentialFormState>) => void;
  errors: CredentialErrors;
  hasSavedCredential: boolean;
}): JSX.Element;

// Plan 3 calls this at save time. Azure returns the trimmed connection label.
// Every other provider returns the trimmed typed scope. No connection returns the trimmed typed scope.
export function vaultNameForSave(
  connection: SecretManagerConnection | undefined,
  typedScope: string,
): string;

// Kind-specific placeholders, exported for tests and for plan 3 if it needs them.
export function credentialEnvPlaceholder(kind: AgentKindValue): string;
export function sourceEnvPlaceholder(kind: AgentKindValue): string;

// Also exported, exactly as in the index contract (re-exported from the modules of task 1).
export const EMPTY_CREDENTIAL_FORM: CredentialFormState;
export const SCOPE_DEBOUNCE_MS: number;
```

Behaviour notes the code below implements:

- Source picker: a `FormField` with `group`, label `Source` and always-visible help, around a `SegmentedControl` labelled Secret manager, Environment, Keychain, `aria-label` `Credential source`. Leaving Keychain clears `localCredential` and `removeSavedCredential`.
- Connection select: every connection as `label (Provider display name)`. A provider whose descriptor has `selectable: false` is listed disabled with ` - not available yet`.
- Scope field only for a selectable, non-Azure connection, labelled and placeholdered from the descriptor. For Azure the connection label is written to `vaultName` on selection.
- The secret list auto-loads when the connection is usable and the effective scope (`vaultNameForSave`) is not empty. A `RefreshCw` icon `Button` with a Tooltip and `aria-label` `Refresh secrets` sits beside the Secret select.
- Changing the connection clears the chosen secret. Changing the scope keeps it. A chosen secret missing from a ready list shows a warning and stays chosen. It stays visible in the select as an extra option.
- `StateBlock` covers loading, empty (no connections, no secrets) and error (with Retry) for both the connection query and the secret listing.
- The section wraps its content in a `TooltipProvider` so it works without an app level provider.

- [ ] **Step 1: Write the test harness**

Create `src/components/settings/agent-config/__tests__/section-harness.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { useCallback, useState } from 'react';
import type { SecretManagerConnection } from '@/lib/tauri-api';
import { AgentCredentialSection } from '../AgentCredentialSection';
import {
  type AgentKindValue,
  type CredentialErrors,
  type CredentialFormState,
  EMPTY_CREDENTIAL_FORM,
} from '../form-types';

// Radix Select calls browser APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.releasePointerCapture) {
  Element.prototype.releasePointerCapture = () => undefined;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

export const CONNECTIONS = [
  { id: 'rv', label: 'Prod vault', provider: 'rocketvault' },
  { id: 'az', label: 'Team Azure', provider: 'azure' },
  { id: 'aws1', label: 'Legacy AWS', provider: 'aws' },
] as SecretManagerConnection[];

interface Options {
  kind?: AgentKindValue;
  initial?: Partial<CredentialFormState>;
  errors?: CredentialErrors;
  hasSavedCredential?: boolean;
}

// Renders the section in a small stateful parent. `latest.current` holds the form state.
export function renderSection(options: Options = {}) {
  const latest: { current: CredentialFormState } = {
    current: { ...EMPTY_CREDENTIAL_FORM, ...options.initial },
  };
  function Harness() {
    const [value, setValue] = useState<CredentialFormState>(latest.current);
    const onChange = useCallback((patch: Partial<CredentialFormState>) => {
      latest.current = { ...latest.current, ...patch };
      setValue(latest.current);
    }, []);
    return (
      <AgentCredentialSection
        kind={options.kind ?? 'claude'}
        value={value}
        onChange={onChange}
        errors={options.errors ?? {}}
        hasSavedCredential={options.hasSavedCredential ?? false}
      />
    );
  }
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const utils = render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
  return { latest, ...utils };
}
```

- [ ] **Step 2: Write the failing vault tests**

Create `src/components/settings/agent-config/__tests__/AgentCredentialSection.vault.test.tsx`:

```tsx
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import {
  EMPTY_CREDENTIAL_FORM,
  SCOPE_DEBOUNCE_MS,
  vaultNameForSave,
} from '../AgentCredentialSection';
import { CONNECTIONS, renderSection } from './section-harness';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  fetchExternalSecretNames: vi.fn(),
  listSecretManagerConnections: vi.fn(),
}));

const fetchMock = () => vi.mocked(api.fetchExternalSecretNames);

const SAVED_RV = {
  vaultConnectionId: 'rv',
  vaultName: 'prod',
  vaultSecretId: 's1',
  vaultSecretName: 'anthropic',
};

async function openConnectionList(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole('combobox', { name: 'Connection' }));
}

describe('vaultNameForSave', () => {
  it('returns the trimmed connection label for Azure', () => {
    const azure = CONNECTIONS[1];
    expect(vaultNameForSave(azure, 'ignored')).toBe('Team Azure');
  });

  it('returns the trimmed typed scope for other providers', () => {
    expect(vaultNameForSave(CONNECTIONS[0], '  prod  ')).toBe('prod');
  });

  it('returns the trimmed typed scope without a connection', () => {
    expect(vaultNameForSave(undefined, ' x ')).toBe('x');
  });
});

describe('AgentCredentialSection contract exports', () => {
  it('re-exports the form default and the debounce delay named in the index', () => {
    expect(EMPTY_CREDENTIAL_FORM.credentialSource).toBe('vault');
    expect(SCOPE_DEBOUNCE_MS).toBe(400);
  });
});

describe('AgentCredentialSection, secret manager source', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue(CONNECTIONS);
    fetchMock().mockResolvedValue([]);
  });

  it('names the source picker group Source and always shows its help', async () => {
    renderSection();
    expect(await screen.findByRole('group', { name: 'Source' })).toBeInTheDocument();
    expect(
      screen.getByText(/Where Rocket gets the key when an agent session starts/),
    ).toBeVisible();
    expect(screen.getByRole('radio', { name: 'Keychain' })).toBeInTheDocument();
  });

  it('lists every connection with its provider and disables unavailable ones', async () => {
    const user = userEvent.setup();
    renderSection();
    await openConnectionList(user);
    expect(screen.getByRole('option', { name: 'Prod vault (RocketVault)' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'Team Azure (Azure Key Vault)' })).toBeInTheDocument();
    const aws = screen.getByRole('option', { name: /Legacy AWS \(AWS Secrets Manager\)/ });
    expect(aws).toHaveAttribute('aria-disabled', 'true');
    expect(aws).toHaveTextContent('not available yet');
  });

  it('shows a scope field from the provider descriptor and loads after typing', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection();
    await openConnectionList(user);
    await user.click(screen.getByRole('option', { name: 'Prod vault (RocketVault)' }));
    expect(latest.current.vaultConnectionId).toBe('rv');
    const scope = await screen.findByLabelText('Vault Name');
    expect(scope).toHaveAttribute('placeholder', 'Vault name');
    expect(fetchMock()).not.toHaveBeenCalled();
    await user.type(scope, 'prod');
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledWith('rv', 'prod'));
    expect(fetchMock()).toHaveBeenCalledTimes(1);
  });

  it('hides the scope for Azure, saves the label as the vault name and loads at once', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection();
    await openConnectionList(user);
    await user.click(screen.getByRole('option', { name: 'Team Azure (Azure Key Vault)' }));
    expect(latest.current.vaultName).toBe('Team Azure');
    expect(screen.queryByLabelText('Vault name')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Vault Name')).not.toBeInTheDocument();
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledWith('az', 'Team Azure'));
  });

  it('clears the chosen secret when the connection changes', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({ initial: SAVED_RV });
    await openConnectionList(user);
    await user.click(screen.getByRole('option', { name: 'Team Azure (Azure Key Vault)' }));
    expect(latest.current.vaultSecretId).toBe('');
    expect(latest.current.vaultSecretName).toBe('');
  });

  it('keeps the chosen secret while the scope changes', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({ initial: SAVED_RV });
    await user.type(await screen.findByLabelText('Vault Name'), 'x');
    expect(latest.current.vaultName).toBe('prodx');
    expect(latest.current.vaultSecretId).toBe('s1');
    expect(latest.current.vaultSecretName).toBe('anthropic');
  });

  it('auto-loads the saved vault and picks a secret from the list', async () => {
    const user = userEvent.setup();
    fetchMock().mockResolvedValue([
      { name: 'anthropic', secretId: 's1' },
      { name: 'other', secretId: 's2' },
    ]);
    const { latest } = renderSection({ initial: SAVED_RV });
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledWith('rv', 'prod'));
    await user.click(await screen.findByRole('combobox', { name: 'Secret' }));
    await user.click(await screen.findByRole('option', { name: 'other' }));
    expect(latest.current.vaultSecretId).toBe('s2');
    expect(latest.current.vaultSecretName).toBe('other');
  });

  it('refreshes the list from the icon button', async () => {
    const user = userEvent.setup();
    renderSection({ initial: SAVED_RV });
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledTimes(1));
    await user.click(await screen.findByRole('button', { name: 'Refresh secrets' }));
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledTimes(2));
  });

  it('warns when the chosen secret is missing from the list and keeps it', async () => {
    fetchMock().mockResolvedValue([{ name: 'other', secretId: 's2' }]);
    const { latest } = renderSection({ initial: SAVED_RV });
    expect(await screen.findByText(/anthropic.*no longer in this list/)).toBeInTheDocument();
    expect(latest.current.vaultSecretId).toBe('s1');
  });

  it('shows a loading block while the secrets load', async () => {
    fetchMock().mockReturnValue(new Promise(() => undefined));
    renderSection({ initial: SAVED_RV });
    expect(await screen.findByText('Loading secrets…')).toBeInTheDocument();
  });

  it('shows an empty state when the vault has no secrets', async () => {
    renderSection({ initial: { ...SAVED_RV, vaultSecretId: '', vaultSecretName: '' } });
    expect(await screen.findByText(/No secrets found/)).toBeInTheDocument();
  });

  it('shows an error with Retry and recovers', async () => {
    const user = userEvent.setup();
    fetchMock().mockRejectedValueOnce('boom');
    fetchMock().mockResolvedValue([{ name: 'anthropic', secretId: 's1' }]);
    renderSection({ initial: SAVED_RV });
    expect(await screen.findByText(/boom/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(screen.queryByText(/boom/)).not.toBeInTheDocument());
    expect(fetchMock()).toHaveBeenCalledTimes(2);
  });

  it('shows an empty state when there are no connections', async () => {
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
    renderSection();
    expect(await screen.findByText(/No secret manager connections yet/)).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Environment' })).toBeInTheDocument();
  });

  it('shows an error with Retry when the connections fail to load', async () => {
    const user = userEvent.setup();
    vi.mocked(api.listSecretManagerConnections).mockRejectedValueOnce('down');
    renderSection();
    expect(await screen.findByText(/Could not load secret manager connections/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByRole('combobox', { name: 'Connection' })).toBeInTheDocument();
  });

  it('warns and does not fetch when the saved connection was deleted', async () => {
    renderSection({ initial: { ...SAVED_RV, vaultConnectionId: 'gone' } });
    expect(await screen.findByText(/saved connection no longer exists/)).toBeInTheDocument();
    expect(fetchMock()).not.toHaveBeenCalled();
  });

  it('warns and does not fetch when the saved connection has no fetcher', async () => {
    renderSection({ initial: { ...SAVED_RV, vaultConnectionId: 'aws1' } });
    expect(await screen.findByText(/AWS Secrets Manager is not available yet/)).toBeInTheDocument();
    expect(fetchMock()).not.toHaveBeenCalled();
  });

  it('shows the field errors it is given', async () => {
    renderSection({
      initial: SAVED_RV,
      errors: { vaultConnectionId: 'Pick a connection.', vaultSecretId: 'Pick a secret.' },
    });
    expect(await screen.findByText('Pick a connection.')).toBeInTheDocument();
    expect(await screen.findByText('Pick a secret.')).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run the tests and expect FAIL**

Run: `yarn test src/components/settings/agent-config/__tests__/AgentCredentialSection.vault.test.tsx`

Expected: FAIL, `../AgentCredentialSection` cannot be resolved.

- [ ] **Step 4: Implement `AgentCredentialSection.tsx`**

Create `src/components/settings/agent-config/AgentCredentialSection.tsx`:

```tsx
import { AlertTriangle, RefreshCw } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';
import { SecretField } from '@/components/ui/secret-field';
import { SegmentedControl } from '@/components/ui/segmented-control';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { StateBlock } from '@/components/ui/state-block';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import { getProviderDescriptor } from '@/lib/secret-providers';
import type { SecretManagerConnection } from '@/lib/tauri-api';
import type { AgentKindValue, CredentialErrors, CredentialFormState } from './form-types';
import { useSecretListing } from './use-secret-listing';

// Re-exported so plan 3 imports the whole credential contract from one module.
export { EMPTY_CREDENTIAL_FORM } from './form-types';
export { SCOPE_DEBOUNCE_MS } from './use-secret-listing';

type CredentialSourceValue = CredentialFormState['credentialSource'];
type Patch = Partial<CredentialFormState>;

const SOURCE_OPTIONS: { value: CredentialSourceValue; label: string }[] = [
  { value: 'vault', label: 'Secret manager' },
  { value: 'env', label: 'Environment' },
  { value: 'keychain', label: 'Keychain' },
];

// Azure ignores the scope, so the connection label stands in for the vault name.
export function vaultNameForSave(
  connection: SecretManagerConnection | undefined,
  typedScope: string,
): string {
  if (connection?.provider === 'azure') return connection.label.trim();
  return typedScope.trim();
}

export function credentialEnvPlaceholder(kind: AgentKindValue): string {
  if (kind === 'gemini') return 'GEMINI_API_KEY';
  if (kind === 'generic') return 'MY_AGENT_API_KEY';
  return 'ANTHROPIC_API_KEY';
}

export function sourceEnvPlaceholder(kind: AgentKindValue): string {
  if (kind === 'gemini') return 'GEMINI_API_KEY';
  if (kind === 'generic') return 'MY_AGENT_API_KEY';
  return 'CLAUDE_CODE_OAUTH_TOKEN';
}

function Warning({ children }: { children: ReactNode }) {
  return (
    <p role='status' className='flex items-start gap-1.5 text-xs text-foreground'>
      <AlertTriangle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-warning' aria-hidden='true' />
      <span>{children}</span>
    </p>
  );
}

interface VaultFieldsProps {
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
}

function VaultFields({ value, onChange, errors }: VaultFieldsProps) {
  const { data: connections = [], isLoading, isError, refetch } = useSecretManagerConnections();
  const connection = connections.find((c) => c.id === value.vaultConnectionId);
  const descriptor = getProviderDescriptor(connection?.provider);
  const isAzure = connection?.provider === 'azure';
  const usable = connection !== undefined && descriptor.selectable;
  const scope = vaultNameForSave(connection, value.vaultName);
  const listing = useSecretListing({
    connectionId: value.vaultConnectionId,
    scope,
    enabled: usable && scope !== '',
  });

  if (isLoading) {
    return <StateBlock tone='loading'>Loading secret manager connections…</StateBlock>;
  }
  if (isError) {
    return (
      <StateBlock tone='error' action={{ label: 'Retry', onClick: () => void refetch() }}>
        Could not load secret manager connections. Check the Secret Managers settings, then retry.
      </StateBlock>
    );
  }
  if (connections.length === 0) {
    return (
      <StateBlock tone='empty'>
        No secret manager connections yet. Add one in the Secret Managers settings, then come back
        to choose a secret.
      </StateBlock>
    );
  }

  const handleConnection = (id: string) => {
    const next = connections.find((c) => c.id === id);
    const patch: Patch = { vaultConnectionId: id, vaultSecretId: '', vaultSecretName: '' };
    if (next?.provider === 'azure') patch.vaultName = next.label;
    else if (isAzure) patch.vaultName = '';
    onChange(patch);
  };

  const chosenMissing =
    value.vaultSecretId !== '' && !listing.secrets.some((s) => s.id === value.vaultSecretId);
  const options = chosenMissing
    ? [
        { id: value.vaultSecretId, name: value.vaultSecretName || value.vaultSecretId },
        ...listing.secrets,
      ]
    : listing.secrets;
  const missingName = value.vaultSecretName || value.vaultSecretId;
  const listActive = usable && scope !== '';
  const deleted = value.vaultConnectionId !== '' && connection === undefined;

  const connectionHelp = isAzure
    ? 'The saved secret manager connection that holds the credential. Azure uses the vault set on the connection, so no vault name is needed.'
    : 'The saved secret manager connection that holds the credential. Add or edit connections in the Secret Managers settings.';

  return (
    <div className='space-y-4'>
      <FormField
        id='ac-vault-connection'
        label='Connection'
        help={connectionHelp}
        error={errors.vaultConnectionId}
      >
        {(a11y) => (
          <Select value={value.vaultConnectionId} onValueChange={handleConnection}>
            <SelectTrigger {...a11y} className='h-8 text-sm'>
              <SelectValue placeholder='Select a connection…' />
            </SelectTrigger>
            <SelectContent>
              {connections.map((c) => {
                const provider = getProviderDescriptor(c.provider);
                const suffix = provider.selectable ? '' : ' - not available yet';
                return (
                  <SelectItem key={c.id} value={c.id} disabled={!provider.selectable}>
                    {`${c.label} (${provider.label})${suffix}`}
                  </SelectItem>
                );
              })}
            </SelectContent>
          </Select>
        )}
      </FormField>
      {deleted && <Warning>The saved connection no longer exists. Pick another one.</Warning>}
      {connection && !descriptor.selectable && (
        <Warning>{`${descriptor.label} is not available yet. Pick another connection.`}</Warning>
      )}
      {usable && !isAzure && (
        <FormField
          id='ac-vault-scope'
          label={descriptor.scopeLabel}
          help={`The ${descriptor.scopeLabel.toLowerCase()} to list secrets from. The list loads once it is filled in, and a wrong value gives an empty list or an error.`}
          error={errors.vaultName}
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.vaultName}
              onChange={(e) => onChange({ vaultName: e.target.value })}
              placeholder={descriptor.scopePlaceholder}
              className='h-8 text-sm'
              autoComplete='off'
              spellCheck={false}
            />
          )}
        </FormField>
      )}
      {listActive && (
        <>
          <FormField
            id='ac-vault-secret'
            label='Secret'
            help='The secret whose value the agent receives. The list loads on its own. Use the refresh button after you create a new secret.'
            error={errors.vaultSecretId}
          >
            {(a11y) => (
              <div className='flex items-center gap-2'>
                <Select
                  value={value.vaultSecretId}
                  onValueChange={(id) => {
                    const picked = options.find((s) => s.id === id);
                    onChange({ vaultSecretId: id, vaultSecretName: picked?.name ?? '' });
                  }}
                >
                  <SelectTrigger {...a11y} className='h-8 flex-1 text-sm'>
                    <SelectValue placeholder='Select a secret…' />
                  </SelectTrigger>
                  <SelectContent>
                    {options.map((s) => (
                      <SelectItem key={s.id} value={s.id}>
                        {s.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant='outline'
                      size='icon'
                      aria-label='Refresh secrets'
                      disabled={listing.status === 'loading'}
                      onClick={listing.refresh}
                    >
                      <RefreshCw
                        className={`h-3.5 w-3.5${listing.status === 'loading' ? ' animate-spin' : ''}`}
                        aria-hidden='true'
                      />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Refresh secrets</TooltipContent>
                </Tooltip>
              </div>
            )}
          </FormField>
          {listing.status === 'loading' && <StateBlock tone='loading'>Loading secrets…</StateBlock>}
          {listing.status === 'error' && (
            <StateBlock tone='error' action={{ label: 'Retry', onClick: listing.refresh }}>
              {`Could not load secrets: ${listing.error ?? 'unknown error'}`}
            </StateBlock>
          )}
          {listing.status === 'ready' && listing.secrets.length === 0 && (
            <StateBlock tone='empty'>
              No secrets found here. Create one in the secret manager, then use the refresh button.
            </StateBlock>
          )}
          {listing.status === 'ready' && chosenMissing && (
            <Warning>
              {`The chosen secret ${missingName} is no longer in this list. Pick another one, or refresh if it was just created.`}
            </Warning>
          )}
        </>
      )}
    </div>
  );
}

interface EnvFieldsProps {
  kind: AgentKindValue;
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
  onTouch: () => void;
}

function EnvFields({ kind, value, onChange, errors, onTouch }: EnvFieldsProps) {
  const source = value.sourceEnvVar.trim();
  const agent = value.credentialEnvVar.trim();
  const shellNote =
    'It must be exported in the shell Rocket was started from. An app started from a desktop icon may not see it.';
  const help =
    source && agent
      ? `Rocket reads ${source} and gives it to the agent as ${agent}. ${shellNote}`
      : `The NAME of the variable in Rocket's own environment that holds the secret. ${shellNote}`;
  return (
    <FormField id='ac-source-env-var' label='Rocket reads it from' help={help} error={errors.sourceEnvVar}>
      {(a11y) => (
        <Input
          {...a11y}
          value={value.sourceEnvVar}
          onChange={(e) => {
            onTouch();
            onChange({ sourceEnvVar: e.target.value });
          }}
          placeholder={sourceEnvPlaceholder(kind)}
          className='h-8 text-sm'
          autoComplete='off'
          spellCheck={false}
        />
      )}
    </FormField>
  );
}

interface KeychainFieldsProps {
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
  hasSavedCredential: boolean;
}

function KeychainFields({ value, onChange, errors, hasSavedCredential }: KeychainFieldsProps) {
  const keepSaved = hasSavedCredential && !value.removeSavedCredential;
  const help = keepSaved
    ? 'A credential is stored in the OS keychain on this computer. Use Replace to change it or Remove to delete it.'
    : 'Stored in the OS keychain on this computer. It is never written to a file and never shown again.';
  // With a saved credential there is no single input to label, so the field is a group.
  const isGroup = hasSavedCredential || value.removeSavedCredential;
  return (
    <FormField
      id='ac-local-credential'
      label='Keychain credential'
      group={isGroup}
      help={help}
      error={errors.localCredential}
    >
      {(a11y) => (
        <SecretField
          id={a11y.id}
          aria-invalid={a11y['aria-invalid']}
          aria-describedby={a11y['aria-describedby']}
          value={value.localCredential}
          onValueChange={(next) => onChange({ localCredential: next })}
          hasSaved={hasSavedCredential}
          markedForRemoval={value.removeSavedCredential}
          onToggleRemove={() =>
            onChange({ removeSavedCredential: !value.removeSavedCredential, localCredential: '' })
          }
          placeholder='Paste the token'
        />
      )}
    </FormField>
  );
}

interface AgentCredentialSectionProps {
  kind: AgentKindValue;
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
  hasSavedCredential: boolean;
}

export function AgentCredentialSection({
  kind,
  value,
  onChange,
  errors,
  hasSavedCredential,
}: AgentCredentialSectionProps) {
  // A saved, different value counts as already edited, so it is never overwritten.
  const [sourceTouched, setSourceTouched] = useState(
    () => value.sourceEnvVar.trim() !== '' && value.sourceEnvVar !== value.credentialEnvVar,
  );

  // Until the user edits it, "Rocket reads it from" follows "Agent reads it as".
  useEffect(() => {
    if (value.credentialSource !== 'env' || sourceTouched) return;
    if (value.sourceEnvVar !== value.credentialEnvVar) {
      onChange({ sourceEnvVar: value.credentialEnvVar });
    }
  }, [value.credentialSource, value.credentialEnvVar, value.sourceEnvVar, sourceTouched, onChange]);

  const handleSource = (next: CredentialSourceValue) => {
    onChange({
      credentialSource: next,
      ...(next === 'keychain' ? {} : { localCredential: '', removeSavedCredential: false }),
    });
  };

  return (
    <TooltipProvider>
      <div className='space-y-4'>
        <FormField
          id='ac-source'
          label='Source'
          group
          help="Where Rocket gets the key when an agent session starts. Secret manager reads it from a connected manager, Environment reads a variable from Rocket's own environment, Keychain stores it in your OS keychain."
        >
          {() => (
            <SegmentedControl
              aria-label='Credential source'
              value={value.credentialSource}
              onValueChange={handleSource}
              options={SOURCE_OPTIONS}
            />
          )}
        </FormField>
        {value.credentialSource === 'vault' && (
          <VaultFields value={value} onChange={onChange} errors={errors} />
        )}
        {value.credentialSource === 'env' && (
          <EnvFields
            kind={kind}
            value={value}
            onChange={onChange}
            errors={errors}
            onTouch={() => setSourceTouched(true)}
          />
        )}
        {value.credentialSource === 'keychain' && (
          <KeychainFields
            value={value}
            onChange={onChange}
            errors={errors}
            hasSavedCredential={hasSavedCredential}
          />
        )}
        <FormField
          id='ac-credential-env-var'
          label='Agent reads it as'
          help={
            <>
              <span className='block'>
                The NAME of the environment variable the agent reads its key from, not the key
                itself. Examples: ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN. The secret comes from
                the source above.
              </span>
              <span className='block'>
                If the credential is a short-lived token, such as an OAuth access token, sessions
                may need a restart after it expires. The value is only read when a new agent
                session starts.
              </span>
            </>
          }
          error={errors.credentialEnvVar}
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.credentialEnvVar}
              onChange={(e) => onChange({ credentialEnvVar: e.target.value })}
              placeholder={credentialEnvPlaceholder(kind)}
              className='h-8 text-sm'
              autoComplete='off'
              spellCheck={false}
            />
          )}
        </FormField>
      </div>
    </TooltipProvider>
  );
}
```

Notes for the implementer: if plan 1's `SecretField` or `FormField` differ slightly from the index signatures, follow the index. If `text-warning` is not a Tailwind colour in this repo, check `tailwind.config` for the `warning` token mapping (`--warning` exists in `src/globals.css`) and use the mapped class.

- [ ] **Step 5: Run the tests and expect PASS**

Run: `yarn test src/components/settings/agent-config/__tests__/AgentCredentialSection.vault.test.tsx`

Expected: PASS, all tests in the file. If a Radix Select option query fails in jsdom, confirm the polyfills in `section-harness.tsx` are loaded (the harness is imported first by the test).

- [ ] **Step 6: Type check and lint**

Run: `yarn tsc --noEmit && yarn check`

Expected: no errors. Fix formatting with `yarn format` on the three files if needed.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with this pathspec commit:

```bash
git add src/components/settings/agent-config/AgentCredentialSection.tsx src/components/settings/agent-config/__tests__/section-harness.tsx src/components/settings/agent-config/__tests__/AgentCredentialSection.vault.test.tsx
git commit --only -m "feat(settings): add agent credential section with secret manager source" -- src/components/settings/agent-config/AgentCredentialSection.tsx src/components/settings/agent-config/__tests__/section-harness.tsx src/components/settings/agent-config/__tests__/AgentCredentialSection.vault.test.tsx
```

---

### Task 3: Environment and Keychain sources, "Agent reads it as"

**Files:**
- Create: `src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx`
- Modify: `src/components/settings/agent-config/AgentCredentialSection.tsx` only if a test below fails (the implementation shipped in Task 2; this task pins and, where needed, fixes it)

**Interfaces:**

Consumes (from Task 2):

```ts
export function AgentCredentialSection(props: { kind: AgentKindValue; value: CredentialFormState; onChange: (patch: Partial<CredentialFormState>) => void; errors: CredentialErrors; hasSavedCredential: boolean }): JSX.Element;
export function credentialEnvPlaceholder(kind: AgentKindValue): string;
export function sourceEnvPlaceholder(kind: AgentKindValue): string;
// section-harness.tsx
export function renderSection(options?: { kind?: AgentKindValue; initial?: Partial<CredentialFormState>; errors?: CredentialErrors; hasSavedCredential?: boolean }): { latest: { current: CredentialFormState } } & RenderResult;
```

Produces: no new exports. This task fixes the behaviour of the Environment source (prefill and live mapping sentence), the Keychain source (`SecretField`), the shared "Agent reads it as" field (every source, short-lived note only there) and the kind-specific placeholders.

Assumption about plan 1: `SecretField` puts `id` on its input and offers Replace and Remove text buttons. The Keychain tests find the input by the label `Keychain credential` and the remove action by `getByRole('button', { name: /remove/i })`. If plan 1 named them differently, adjust only these queries, not the contract.

- [ ] **Step 1: Write the failing tests**

Create `src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { credentialEnvPlaceholder, sourceEnvPlaceholder } from '../AgentCredentialSection';
import { CONNECTIONS, renderSection } from './section-harness';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  fetchExternalSecretNames: vi.fn(),
  listSecretManagerConnections: vi.fn(),
}));

describe('placeholders', () => {
  it.each([
    ['claude', 'ANTHROPIC_API_KEY', 'CLAUDE_CODE_OAUTH_TOKEN'],
    ['gemini', 'GEMINI_API_KEY', 'GEMINI_API_KEY'],
    ['generic', 'MY_AGENT_API_KEY', 'MY_AGENT_API_KEY'],
  ] as const)('uses kind specific examples for %s', (kind, agent, source) => {
    expect(credentialEnvPlaceholder(kind)).toBe(agent);
    expect(sourceEnvPlaceholder(kind)).toBe(source);
  });
});

describe('AgentCredentialSection, environment and keychain sources', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue(CONNECTIONS);
    vi.mocked(api.fetchExternalSecretNames).mockResolvedValue([]);
  });

  it('offers the three sources with the spec labels', async () => {
    renderSection();
    expect(await screen.findByRole('radio', { name: 'Secret manager' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Environment' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Keychain' })).toBeInTheDocument();
  });

  it('switches the source and clears the typed keychain value when leaving keychain', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({
      initial: { credentialSource: 'keychain', localCredential: 'tok', removeSavedCredential: true },
    });
    await user.click(screen.getByRole('radio', { name: 'Environment' }));
    expect(latest.current.credentialSource).toBe('env');
    expect(latest.current.localCredential).toBe('');
    expect(latest.current.removeSavedCredential).toBe(false);
  });

  it('prefills "Rocket reads it from" from "Agent reads it as" until it is edited', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({
      kind: 'gemini',
      initial: { credentialSource: 'env', credentialEnvVar: 'GEMINI_API_KEY' },
    });
    const from = screen.getByLabelText('Rocket reads it from');
    expect(from).toHaveValue('GEMINI_API_KEY');
    expect(latest.current.sourceEnvVar).toBe('GEMINI_API_KEY');

    const agent = screen.getByLabelText('Agent reads it as');
    await user.type(agent, '_2');
    expect(latest.current.sourceEnvVar).toBe('GEMINI_API_KEY_2');

    await user.clear(from);
    await user.type(from, 'MY_KEY');
    await user.type(agent, 'X');
    expect(latest.current.credentialEnvVar).toBe('GEMINI_API_KEY_2X');
    expect(latest.current.sourceEnvVar).toBe('MY_KEY');
  });

  it('never overwrites a saved source name that differs from the agent name', () => {
    const { latest } = renderSection({
      initial: {
        credentialSource: 'env',
        credentialEnvVar: 'ANTHROPIC_API_KEY',
        sourceEnvVar: 'MY_KEY',
      },
    });
    expect(latest.current.sourceEnvVar).toBe('MY_KEY');
    expect(screen.getByLabelText('Rocket reads it from')).toHaveValue('MY_KEY');
  });

  it('spells out the mapping in the help text and notes the shell requirement', () => {
    renderSection({
      kind: 'gemini',
      initial: {
        credentialSource: 'env',
        credentialEnvVar: 'GEMINI_API_KEY',
        sourceEnvVar: 'MY_KEY',
      },
    });
    expect(
      screen.getByText(/Rocket reads MY_KEY and gives it to the agent as GEMINI_API_KEY/),
    ).toBeInTheDocument();
    expect(screen.getByText(/exported in the shell Rocket was started from/)).toBeInTheDocument();
  });

  it('falls back to a general sentence while a name is empty', () => {
    renderSection({ initial: { credentialSource: 'env' } });
    expect(screen.queryByText(/gives it to the agent as/)).not.toBeInTheDocument();
    expect(screen.getByText(/NAME of the variable in Rocket's own environment/)).toBeInTheDocument();
  });

  it('shows the exported-in-the-shell note only for the environment source', async () => {
    renderSection({ initial: { credentialSource: 'keychain' } });
    expect(screen.queryByText(/exported in the shell/)).not.toBeInTheDocument();
  });

  it.each(['vault', 'env', 'keychain'] as const)(
    'shows "Agent reads it as" with its help once for the %s source',
    async (credentialSource) => {
      renderSection({ initial: { credentialSource } });
      expect(await screen.findByLabelText('Agent reads it as')).toBeInTheDocument();
      expect(screen.getByText(/not the key itself/)).toBeInTheDocument();
      expect(screen.getByText(/ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN/)).toBeInTheDocument();
      expect(screen.getAllByText(/short-lived token/)).toHaveLength(1);
    },
  );

  it.each([
    ['claude', 'ANTHROPIC_API_KEY'],
    ['gemini', 'GEMINI_API_KEY'],
    ['generic', 'MY_AGENT_API_KEY'],
  ] as const)('uses the %s placeholder for "Agent reads it as"', async (kind, placeholder) => {
    renderSection({ kind, initial: { credentialSource: 'keychain' } });
    expect(await screen.findByLabelText('Agent reads it as')).toHaveAttribute(
      'placeholder',
      placeholder,
    );
  });

  it('uses the kind specific placeholder for "Rocket reads it from"', () => {
    renderSection({ kind: 'claude', initial: { credentialSource: 'env', credentialEnvVar: '' } });
    expect(screen.getByLabelText('Rocket reads it from')).toHaveAttribute(
      'placeholder',
      'CLAUDE_CODE_OAUTH_TOKEN',
    );
  });

  it('shows the errors for both names', () => {
    renderSection({
      initial: { credentialSource: 'env' },
      errors: { credentialEnvVar: 'Bad agent name.', sourceEnvVar: 'Bad source name.' },
    });
    expect(screen.getByText('Bad agent name.')).toBeInTheDocument();
    expect(screen.getByText('Bad source name.')).toBeInTheDocument();
    expect(screen.getByLabelText('Agent reads it as')).toHaveAttribute('aria-invalid', 'true');
  });

  it('edits the keychain credential through the secret field', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({ initial: { credentialSource: 'keychain' } });
    await user.type(screen.getByLabelText('Keychain credential'), 'tok');
    expect(latest.current.localCredential).toBe('tok');
    expect(screen.getByText(/stored in the OS keychain/i)).toBeInTheDocument();
    expect(screen.queryByText('Saved in the keychain')).not.toBeInTheDocument();
  });

  it('explains a saved keychain credential and marks it for removal', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({
      initial: { credentialSource: 'keychain' },
      hasSavedCredential: true,
    });
    // The pill text is an exact match and appears once. The help uses different words.
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
    expect(
      screen.getByText(/A credential is stored in the OS keychain on this computer/i),
    ).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Keychain credential' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /remove/i }));
    expect(latest.current.removeSavedCredential).toBe(true);
    expect(latest.current.localCredential).toBe('');
    expect(screen.getByText(/never written to a file/i)).toBeInTheDocument();
  });

  it('shows the keychain error', () => {
    renderSection({
      initial: { credentialSource: 'keychain' },
      errors: { localCredential: 'Enter the credential to save.' },
    });
    expect(screen.getByText('Enter the credential to save.')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests**

Run: `yarn test src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx`

Expected: most tests PASS already because Task 2 shipped the real components. Any FAIL is a genuine gap. The one test expected to need a change if the Task 2 text drifted is `shows the exported-in-the-shell note only for the environment source`. If every test passes, the red phase is the mere absence of this file, so confirm the file was missing before this step (`git status` shows it as new) and continue.

- [ ] **Step 3: Fix any failing behaviour in `AgentCredentialSection.tsx`**

Make the minimum edit for each failing test, using these exact expectations:

1. Prefill: the `useEffect` in `AgentCredentialSection` must only patch `sourceEnvVar` when `credentialSource === 'env'`, `sourceTouched` is false and the two names differ. It must never run for `vault` or `keychain`.
2. Mapping sentence: in `EnvFields`, the help is `Rocket reads ${source} and gives it to the agent as ${agent}. ${shellNote}` when both trimmed names are non-empty, otherwise `The NAME of the variable in Rocket's own environment that holds the secret. ${shellNote}`.
3. Short-lived note: it appears only in the `Agent reads it as` help, never in `EnvFields` or `KeychainFields`.
4. Placeholders: `credentialEnvPlaceholder` returns `GEMINI_API_KEY`, `MY_AGENT_API_KEY`, `ANTHROPIC_API_KEY` for gemini, generic, claude. `sourceEnvPlaceholder` returns `GEMINI_API_KEY`, `MY_AGENT_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`.
5. Keychain help: `A credential is stored in the OS keychain on this computer. Use Replace to change it or Remove to delete it.` when a saved credential exists and is not marked for removal, otherwise `Stored in the OS keychain on this computer. It is never written to a file and never shown again.` The phrase `Saved in the keychain` belongs only to the `SecretField` pill and must not appear in any help text.
6. Keychain `FormField` gets `group` when `hasSavedCredential` is true or the credential is marked for removal, because there is then no single input to label. Otherwise it is a normal label for the password input.
7. Warning: a lucide `AlertTriangle` (`aria-hidden`) with `text-warning` on the icon only, and `text-foreground` on the sentence.

If no edit is needed, skip to Step 4.

- [ ] **Step 4: Run the tests and expect PASS**

Run: `yarn test src/components/settings/agent-config`

Expected: PASS for the whole folder (form-types, hook, vault and local section tests).

- [ ] **Step 5: Type check and lint**

Run: `yarn tsc --noEmit && yarn check`

Expected: no errors. Fix formatting with `yarn format` on the touched files if needed.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Run exactly one of the two pathspec commits below.

If Step 3 changed nothing (test file only):

```bash
git add src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx
git commit --only -m "test(settings): cover environment and keychain credential sources" -- src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx
```

If Step 3 changed `AgentCredentialSection.tsx` (test file plus the component):

```bash
git add src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx src/components/settings/agent-config/AgentCredentialSection.tsx
git commit --only -m "fix(settings): align credential section with environment and keychain tests" -- src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx src/components/settings/agent-config/AgentCredentialSection.tsx
```

---

## Handoff notes for plan 3

- Import `vaultNameForSave` from `./AgentCredentialSection` and call `vaultNameForSave(connection, form.vaultName)` when building the `AgentConfig` for save. For a non-vault source, save empty `vault_*` fields as the dialog does today.
- Pass a stable `onChange` (for example `useCallback`) to `AgentCredentialSection`. Its prefill effect lists `onChange` as a dependency. Do not rely on a closure over `value` inside that callback: the section calls `onChange(patch)` with a partial, so merge it with functional state (`setForm((prev) => ({ ...prev, ...patch }))`).
- `AgentCredentialSection.tsx` re-exports `EMPTY_CREDENTIAL_FORM` and `SCOPE_DEBOUNCE_MS`, so plan 3 can import `vaultNameForSave`, `EMPTY_CREDENTIAL_FORM` and `SCOPE_DEBOUNCE_MS` from one module.
- Validation messages come from plan 3's validation module and arrive through the `errors` prop. The section shows them inline and never validates on its own.
- `EMPTY_CREDENTIAL_FORM` is the credential state of a new agent.
