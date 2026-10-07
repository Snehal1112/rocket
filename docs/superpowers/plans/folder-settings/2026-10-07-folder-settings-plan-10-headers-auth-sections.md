# Folder settings, Plan 10: Headers and Auth sub-tabs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Headers and Auth sub-tabs of the Folder Settings tab edit `FolderSettings.headers` and `FolderSettings.auth` using the same editors as the collection tab. A request set to Inherit shows where its authorization comes from, and sending such a request uses the nearest folder auth before the collection auth.

**Architecture:** Both sections replace the placeholders from plan 08 and follow the section props contract from plan 09: `{ collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void }`. The shell's single Save button and dirty flag (plan 09) are untouched, so a section never saves. Each section keeps its editor rows or `AuthState` in local state, because both hold data that does not survive a round trip through the persisted shape (blank header rows, OAuth2 tokens), and pushes the persisted shape up through `onChange`. A new in-memory `folder-auth-store` keeps OAuth2 tokens per collection and folder, written on every edit exactly like `collection-auth-store` is. A small `inherited-auth` module resolves the folder that supplies inherited auth, and is used by both the request-side hint and `execute-request.ts`.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui, lucide-react, Vitest and Testing Library. Run frontend tests with `yarn test --run <path>`.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md` (Frontend and Runtime rules), locked contract in `docs/superpowers/plans/folder-settings/00-plan-index.md`.

## Global Constraints

- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<select>`, `<form>`, `<dialog>`), `lucide-react` icons only, `SingleLineEditor` for single-line fields. Zustand selectors stay narrow; code outside React uses `useXStore.getState()`.
- Reuse `HeadersEditor`, `KeyValueEditor`, `AuthEditor`, `authStateForType`, `toPersistedAuth`, `fromPersistedAuth` and `toPersistedHeaders`. Do not write a second auth or header converter.
- Persistence shapes carry no camelCase rename concerns on the frontend, but nothing in this plan changes Rust. No `unwrap()` anywhere.
- A folder can only add auth, never remove it. The backend `resolve_folder_auth` skips both `None` and `Inherit`, so a folder set to No Auth would still hand its children the parent's auth. The Auth sub-tab therefore offers **Inherit** (stored as no folder auth) and the real auth types, and never offers "None". If `folder.yml` already holds `auth: none` (for example written by Bruno), the selector shows that current value as a read-only "No Auth (set on disk)" entry so the value is not silently rewritten, and a note explains that it has no effect.
- Header `description` is persisted by the backend (`rocket_shared::types::Header.description`) but the editor rows have no description column. Saving must carry the description of each header over by key, otherwise this tab destroys data written by Bruno or by hand.
- OAuth2 access and refresh tokens are never written to `folder.yml` (same rule as the collection). They live only in `folder-auth-store`, which is lost on app restart by design.
- Every frontend task ends with `yarn tsc --noEmit` and `yarn check`. If `yarn check` reports only import order or formatting, run `yarn lint` and re-run `yarn check`.
- Commits use the `dev-workflow-skills:1-git-commit` skill, stage explicit paths only (never `git add -A` or `git add .`), and commit with a pathspec.

## Contract assumptions (verify in Task 1, Step 2)

Plans 04, 08 and 09 are not in this repo yet when this plan is written. The code below assumes:

- `src/lib/tauri-api.ts` exports `type FolderSettings` with at least `headers: Header[]`, `auth: Auth | null`, `variables: CollectionVariable[]`, and `getFolderSettings(collection: string, folderPath: string): Promise<FolderSettings>`.
- `src/components/collections/folder-settings/HeadersSection.tsx` and `AuthSection.tsx` export the named components `HeadersSection` and `AuthSection`.
- Tests that need a `FolderSettings` value build it with `as unknown as FolderSettings`, so they compile whatever the remaining fields (scripts, docs) are typed as.

If any of these differ, adapt the imports and the one cast in the tests, and record the difference in the plan 04 or 09 file.

## Known gaps (not closed by this plan)

1. **Header precedence with collection headers.** Closed by plan 05 task 3, which merges collection and folder headers in `resolveRequestFieldsForPath` so that request beats folder beats collection. Nothing to do here. Plan 05 must run before this plan.
2. **Folder OAuth2 on the backend-only paths.** Flow nodes and any run that does not go through `resolveRequestFieldsForPath` resolve inherited folder auth on the backend (plan 05) from `folder.yml`, which never holds tokens. A folder with OAuth2 therefore sends no token there. Inherited collection OAuth2 has the same limit today. Closing it needs a runtime token source in the backend, which is outside this series.
3. **No auto-fetch or auto-refresh for inherited OAuth2.** `maybeAutoRefreshOrFetchToken` only handles a request's own OAuth2 auth. Inherited collection OAuth2 has the same limit. For a folder, the user clicks Get Token in the folder's Auth sub-tab. Parity with the collection, not a regression.
4. **Token store keys are not renamed with the folder.** `folder-auth-store` is keyed by collection and folder path. Renaming or deleting a folder leaves a stale in-memory entry that is never read again. Harmless, not cleaned up.
5. **Header description in the TS type.** If plan 04's `FolderSettings.headers` is typed as the existing `Header` (`key`, `value`, `enabled` only), `description` still round-trips because the IPC payload carries it and `entriesToHeaders` copies it by key. Plan 04 should add `description?: unknown` or the real type to `Header` so the field is visible.
6. **The request-side hint is not live.** It is computed when the request's Auth tab opens with Inherit selected. Saving a folder's auth does not refresh an already open request tab until it is reopened or the section changes.

## Review Focus

1. Adding a header row with an empty name must not reach `onChange` or dirty the tab (Task 1 test `adding a blank row does not call onChange`).
2. Header descriptions already on disk survive an edit of the same header (Task 1 test `editing a value keeps the header description`).
3. A disabled header stays disabled and is kept (Task 1 test `a disabled header keeps enabled false`).
4. Choosing Inherit stores no folder auth (`null`), and `None` is never offered unless it is already on disk (Task 2 tests `choosing Inherit clears the folder auth` and `does not offer None for a folder`).
5. OAuth2 tokens are restored into the editor from the in-memory store and written to it on every edit, so a tab switch during a browser flow cannot lose them (Task 2 tests `restores a cached OAuth2 token into the editor` and `writes every edit to the folder auth store`).
6. An outside change to `settings.auth` resets the editor, and the section's own echo does not (Task 2 test `resets the editor when settings.auth changes from outside`).
7. Sending a request set to Inherit uses the nearest folder auth before the collection auth, including a cached folder OAuth2 token (Task 3 tests `uses the nearest folder auth for an inheriting request` and `a folder OAuth2 token reaches the wire as a bearer token`).
8. The request Auth tab says which folder or the collection supplies inherited auth (Task 3 tests in `inherited-auth.test.ts` and `useInheritedAuthSource.test.ts`).

---

## Task 1: Headers sub-tab

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/lib/folder-settings-convert.ts`
- Create: `src/hooks/useFolderVariableContext.ts`
- Modify: `src/components/collections/folder-settings/HeadersSection.tsx` (replaces the plan 08 placeholder)
- Test: `src/lib/__tests__/folder-settings-convert.test.ts`
- Test: `src/hooks/__tests__/useFolderVariableContext.test.tsx`
- Test: `src/components/collections/folder-settings/__tests__/HeadersSection.test.tsx`

**Interfaces:**
- Consumes:
  - `Header`, `CollectionVariable`, `FolderSettings`, `getCollectionSettings(collection)`, `getFolderChainVariables(collection, requestPath)` from `@/lib/tauri-api`.
  - `HeadersEditor` props `{ headers: KeyValueEntry[]; onChange; variableContext?; onNavigateToSource? }`.
  - `toPersistedHeaders(headers): Header[]` from `@/lib/persisted-headers` (drops blank keys, keeps `enabled`).
  - `buildScopedContext`, `VariableScopeEntry` from `@/lib/url-variables`.
  - `useEnvironments`, `useGlobalEnvironment`, `useGlobalEnvironmentName`, `useProcessEnvVars` from `@/lib/queries/environment-queries`; `useEnvStore` selectors `activeEnvId`, `activeCollection`.
- Produces:
  - `type FolderHeader = Header & { description?: unknown }`
  - `headersToEntries(headers: readonly Header[]): KeyValueEntry[]`
  - `entriesToHeaders(entries: KeyValueEntry[], previous: readonly FolderHeader[]): FolderHeader[]`
  - `folderChainPath(folderPath: string): string` returning `''` for the root folder and `` `${folderPath}/folder.yml` `` otherwise. It is a synthetic request path whose parent is the folder, so the existing chain-walking commands include the folder itself.
  - `useFolderVariableContext(collectionName: string, folderPath: string, ownVariables: CollectionVariable[]): { variableContext: Map<string, VariableScopeEntry>; environmentName: string | undefined }`
  - `HeadersSection(props: { collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void })`

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md` (headers and auth sections of `folder.yml`).

- [ ] **Step 2: Confirm the contract assumptions**

Run:

```bash
grep -n "FolderSettings\|getFolderSettings\|export interface Header" src/lib/tauri-api.ts
ls src/components/collections/folder-settings/
cat src/components/collections/folder-settings/HeadersSection.tsx src/components/collections/folder-settings/AuthSection.tsx
```

Expected: `FolderSettings` and `getFolderSettings` exist (plan 04), both section files exist as placeholders (plan 08) with named exports. Keep the export style and the props interface the placeholders use. If the placeholders declare a shared props type, import it instead of declaring `HeadersSectionProps` below.

- [ ] **Step 3: Write the failing conversion tests**

Create `src/lib/__tests__/folder-settings-convert.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  entriesToHeaders,
  folderChainPath,
  headersToEntries,
} from '@/lib/folder-settings-convert';

describe('headersToEntries', () => {
  it('maps headers to editor rows with unique ids', () => {
    const rows = headersToEntries([
      { key: 'X-A', value: '1', enabled: true },
      { key: 'X-B', value: '2', enabled: false },
    ]);
    expect(rows.map((r) => [r.key, r.value, r.enabled])).toEqual([
      ['X-A', '1', true],
      ['X-B', '2', false],
    ]);
    expect(new Set(rows.map((r) => r.id)).size).toBe(2);
  });
});

describe('entriesToHeaders', () => {
  it('drops blank-key rows and keeps enabled false', () => {
    const out = entriesToHeaders(
      [
        { id: '0', key: 'X-A', value: '1', enabled: false },
        { id: '1', key: '', value: 'draft', enabled: true },
      ],
      [],
    );
    expect(out).toEqual([{ key: 'X-A', value: '1', enabled: false }]);
  });

  it('carries the description over by key', () => {
    const previous = [{ key: 'X-A', value: 'old', enabled: true, description: 'why' }];
    const out = entriesToHeaders([{ id: '0', key: 'X-A', value: 'new', enabled: true }], previous);
    expect(out).toEqual([{ key: 'X-A', value: 'new', enabled: true, description: 'why' }]);
  });

  it('adds no description key when the header had none', () => {
    const out = entriesToHeaders([{ id: '0', key: 'X-A', value: '1', enabled: true }], [
      { key: 'X-A', value: '1', enabled: true, description: null },
    ]);
    expect('description' in out[0]).toBe(false);
  });
});

describe('folderChainPath', () => {
  it('is empty for the root folder', () => {
    expect(folderChainPath('')).toBe('');
  });

  it('is a synthetic file path inside the folder', () => {
    expect(folderChainPath('api/users')).toBe('api/users/folder.yml');
  });
});
```

- [ ] **Step 4: Run it and see it fail**

Run: `yarn test --run src/lib/__tests__/folder-settings-convert.test.ts`
Expected: FAIL, cannot resolve `@/lib/folder-settings-convert`.

- [ ] **Step 5: Implement the header conversions**

Create `src/lib/folder-settings-convert.ts`:

```ts
// Conversions between the Folder Settings payload and the editor state types.
// Auth conversion reuses persisted-auth.ts, header conversion reuses persisted-headers.ts.
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { Header } from '@/lib/tauri-api';
import type { KeyValueEntry } from '@/types/pane-types';

/** A persisted header, which can carry a description the editor rows do not show. */
export type FolderHeader = Header & { description?: unknown };

/** Editor rows for a folder's headers. Ids are positional, like the collection tab. */
export function headersToEntries(headers: readonly Header[]): KeyValueEntry[] {
  return headers.map((h, i) => ({
    id: String(i),
    key: h.key,
    value: h.value,
    enabled: h.enabled,
  }));
}

/**
 * Persisted headers for the editor rows. Blank-key rows are dropped and `enabled` is kept
 * as is. A description is carried over from `previous` by header name, because the rows
 * have no description column and saving would otherwise erase it.
 */
export function entriesToHeaders(
  entries: KeyValueEntry[],
  previous: readonly FolderHeader[],
): FolderHeader[] {
  const descriptions = new Map<string, unknown>();
  for (const h of previous) {
    if (h.description !== undefined && h.description !== null && !descriptions.has(h.key)) {
      descriptions.set(h.key, h.description);
    }
  }
  return toPersistedHeaders(entries).map((h) => {
    const description = descriptions.get(h.key);
    return description === undefined ? h : { ...h, description };
  });
}

/**
 * A synthetic request path inside the folder. The chain commands walk the parents of a
 * request path, so passing a file in the folder includes the folder itself. The root folder
 * has no parents to walk, so it maps to an empty path.
 */
export function folderChainPath(folderPath: string): string {
  return folderPath ? `${folderPath}/folder.yml` : '';
}
```

- [ ] **Step 6: Run the conversion tests and see them pass**

Run: `yarn test --run src/lib/__tests__/folder-settings-convert.test.ts`
Expected: PASS.

- [ ] **Step 7: Write the failing variable-context hook test**

Create `src/hooks/__tests__/useFolderVariableContext.test.tsx`:

```tsx
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CollectionVariable } from '@/lib/tauri-api';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
  getFolderChainVariables: vi.fn(),
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: () => ({ data: [] }),
  useGlobalEnvironmentName: () => ({ data: null }),
  useGlobalEnvironment: () => ({ data: null }),
  useProcessEnvVars: () => ({ data: {} }),
}));

import { useFolderVariableContext } from '../useFolderVariableContext';

const v = (key: string, value: string): CollectionVariable => ({
  key,
  value,
  initialValue: '',
  enabled: true,
  secret: false,
});

describe('useFolderVariableContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getCollectionSettings.mockResolvedValue({ variables: [v('host', 'c.example')] });
    api.getFolderChainVariables.mockResolvedValue([v('host', 'outer.example'), v('team', 'a')]);
  });

  it('layers collection, saved folder chain and unsaved folder variables', async () => {
    const { result } = renderHook(() =>
      useFolderVariableContext('demo', 'api/users', [v('team', 'b')]),
    );
    await waitFor(() => expect(result.current.variableContext.get('host')?.source).toBe('folder'));
    expect(result.current.variableContext.get('host')?.value).toBe('outer.example');
    expect(result.current.variableContext.get('team')?.value).toBe('b');
    expect(api.getFolderChainVariables).toHaveBeenCalledWith('demo', 'api/users/folder.yml');
  });

  it('skips the chain lookup for the root folder', async () => {
    const { result } = renderHook(() => useFolderVariableContext('demo', '', []));
    await waitFor(() =>
      expect(result.current.variableContext.get('host')?.source).toBe('collection'),
    );
    expect(api.getFolderChainVariables).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 8: Run it and see it fail**

Run: `yarn test --run src/hooks/__tests__/useFolderVariableContext.test.tsx`
Expected: FAIL, cannot resolve `../useFolderVariableContext`.

- [ ] **Step 9: Implement the hook**

Create `src/hooks/useFolderVariableContext.ts`:

```ts
import { useEffect, useMemo, useState } from 'react';
import { folderChainPath } from '@/lib/folder-settings-convert';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import {
  type CollectionVariable,
  getCollectionSettings,
  getFolderChainVariables,
} from '@/lib/tauri-api';
import { buildScopedContext, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

export interface FolderVariableScope {
  /** Scope-aware variables for the editors of a folder's sections. */
  variableContext: Map<string, VariableScopeEntry>;
  environmentName: string | undefined;
}

/**
 * The variable scopes a folder's sections see: process, global and active environment, the
 * collection, the saved folder chain down to this folder, and this folder's unsaved edits.
 * `ownVariables` goes last so an unsaved edit wins over the saved value of the same name.
 */
export function useFolderVariableContext(
  collectionName: string,
  folderPath: string,
  ownVariables: CollectionVariable[],
): FolderVariableScope {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars();

  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);
  const [chainVars, setChainVars] = useState<CollectionVariable[]>([]);

  useEffect(() => {
    getCollectionSettings(collectionName)
      .then((s) => setCollectionVars(s.variables))
      .catch(() => setCollectionVars([]));
  }, [collectionName]);

  useEffect(() => {
    const path = folderChainPath(folderPath);
    if (!path) {
      setChainVars([]);
      return;
    }
    getFolderChainVariables(collectionName, path)
      .then(setChainVars)
      .catch(() => setChainVars([]));
  }, [collectionName, folderPath]);

  const variableContext = useMemo(() => {
    const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
    const envVars: Record<string, string> = {};
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) envVars[v.key] = v.value;
    const globalVars: Record<string, string> = globalEnv
      ? Object.fromEntries(
          globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
        )
      : {};
    return buildScopedContext({
      envVars,
      envLabel: activeEnvId ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars,
      processEnvVars,
      collectionVars,
      folderVars: [...chainVars, ...ownVariables],
    });
  }, [
    activeEnvId,
    environments,
    globalEnv,
    processEnvVars,
    collectionVars,
    chainVars,
    ownVariables,
  ]);

  return { variableContext, environmentName: activeEnvId ?? undefined };
}
```

- [ ] **Step 10: Run the hook test and see it pass**

Run: `yarn test --run src/hooks/__tests__/useFolderVariableContext.test.tsx`
Expected: PASS. `useEnvStore` is the real store, so `activeEnvId` is `null` and the environment is ignored.

- [ ] **Step 11: Write the failing HeadersSection tests**

Create `src/components/collections/folder-settings/__tests__/HeadersSection.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
  }) => <input aria-label={placeholder} value={value} onChange={(e) => onChange(e.target.value)} />,
}));
vi.mock('@/hooks/useFolderVariableContext', () => ({
  useFolderVariableContext: () => ({ variableContext: new Map(), environmentName: undefined }),
}));

import { HeadersSection } from '../HeadersSection';

const settings = (patch: Record<string, unknown> = {}): FolderSettings =>
  ({
    headers: [],
    auth: null,
    variables: [],
    preRequestScript: null,
    postResponseScript: null,
    testsScript: null,
    docs: null,
    ...patch,
  }) as unknown as FolderSettings;

describe('HeadersSection', () => {
  it('shows the folder headers', () => {
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-A', value: '1', enabled: true }] })}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('Key for row 1')).toHaveValue('X-A');
  });

  it('editing a value keeps the header description', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({
          headers: [{ key: 'X-A', value: '1', enabled: true, description: 'why' }],
        })}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Value'), { target: { value: '2' } });
    expect(onChange).toHaveBeenLastCalledWith({
      headers: [{ key: 'X-A', value: '2', enabled: true, description: 'why' }],
    });
  });

  it('a disabled header keeps enabled false', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-A', value: '1', enabled: false }] })}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Value'), { target: { value: '2' } });
    expect(onChange).toHaveBeenLastCalledWith({
      headers: [{ key: 'X-A', value: '2', enabled: false }],
    });
  });

  it('adding a blank row does not call onChange', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings()}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /Add Header/ }));
    expect(screen.getByLabelText('Key for row 1')).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it('naming a new row sends the header', () => {
    const onChange = vi.fn();
    render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings()}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /Add Header/ }));
    fireEvent.change(screen.getByLabelText('Key for row 1'), { target: { value: 'X-New' } });
    expect(onChange).toHaveBeenLastCalledWith({
      headers: [{ key: 'X-New', value: '', enabled: true }],
    });
  });

  it('resets the rows when settings.headers changes from outside', () => {
    const { rerender } = render(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-A', value: '1', enabled: true }] })}
        onChange={vi.fn()}
      />,
    );
    rerender(
      <HeadersSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ headers: [{ key: 'X-Z', value: '9', enabled: true }] })}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('Key for row 1')).toHaveValue('X-Z');
  });
});
```

Note: the mocked `SingleLineEditor` uses the placeholder as its label, and `KeyValueEditor` gives the value editor the placeholder `Value`, so `getByLabelText('Value')` finds the value input of the only row.

- [ ] **Step 12: Run it and see it fail**

Run: `yarn test --run src/components/collections/folder-settings/__tests__/HeadersSection.test.tsx`
Expected: FAIL (the placeholder section renders none of these rows).

- [ ] **Step 13: Implement HeadersSection**

Replace the contents of `src/components/collections/folder-settings/HeadersSection.tsx` (keep the export style found in Step 2):

```tsx
import { useEffect, useState } from 'react';
import { HeadersEditor } from '@/components/request/HeadersEditor';
import { useFolderVariableContext } from '@/hooks/useFolderVariableContext';
import { entriesToHeaders, headersToEntries } from '@/lib/folder-settings-convert';
import type { FolderSettings } from '@/lib/tauri-api';
import type { KeyValueEntry } from '@/types/pane-types';

interface HeadersSectionProps {
  collectionName: string;
  folderPath: string;
  settings: FolderSettings;
  onChange: (patch: Partial<FolderSettings>) => void;
}

export function HeadersSection({
  collectionName,
  folderPath,
  settings,
  onChange,
}: HeadersSectionProps) {
  // Rows live here, not in `settings`, so a row with no name yet is not dropped on the
  // next render. Only complete rows are pushed up.
  const [entries, setEntries] = useState<KeyValueEntry[]>(() =>
    headersToEntries(settings.headers),
  );
  const { variableContext } = useFolderVariableContext(
    collectionName,
    folderPath,
    settings.variables,
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: only an outside change to settings.headers resets the rows; the local rows are compared inside.
  useEffect(() => {
    if (
      JSON.stringify(entriesToHeaders(entries, settings.headers)) !==
      JSON.stringify(settings.headers)
    ) {
      setEntries(headersToEntries(settings.headers));
    }
  }, [settings.headers]);

  const handleChange = (next: KeyValueEntry[]) => {
    setEntries(next);
    const headers = entriesToHeaders(next, settings.headers);
    if (JSON.stringify(headers) === JSON.stringify(settings.headers)) return;
    onChange({ headers });
  };

  return (
    <div className='p-4 max-w-3xl'>
      <p className='mb-3 text-xs text-muted-foreground'>
        These headers are sent with every request in this folder and its subfolders. A request, or
        a folder closer to it, replaces a header with the same name.
      </p>
      <HeadersEditor
        headers={entries}
        onChange={handleChange}
        variableContext={variableContext}
      />
    </div>
  );
}
```

- [ ] **Step 14: Run the section tests and see them pass**

Run: `yarn test --run src/components/collections/folder-settings/__tests__/HeadersSection.test.tsx`
Expected: PASS (6 tests).

- [ ] **Step 15: Type-check and lint**

Run: `yarn tsc --noEmit` then `yarn check`.
Expected: both clean. If `yarn check` only reports import order or formatting, run `yarn lint`, then `yarn check` again.

- [ ] **Step 16: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only, then pathspec-commit:

```bash
git add src/lib/folder-settings-convert.ts src/hooks/useFolderVariableContext.ts src/components/collections/folder-settings/HeadersSection.tsx src/lib/__tests__/folder-settings-convert.test.ts src/hooks/__tests__/useFolderVariableContext.test.tsx src/components/collections/folder-settings/__tests__/HeadersSection.test.tsx
git commit --only -m "feat(folder-settings): add the Headers sub-tab" -- src/lib/folder-settings-convert.ts src/hooks/useFolderVariableContext.ts src/components/collections/folder-settings/HeadersSection.tsx src/lib/__tests__/folder-settings-convert.test.ts src/hooks/__tests__/useFolderVariableContext.test.tsx src/components/collections/folder-settings/__tests__/HeadersSection.test.tsx
```

The commit message comes from the skill, in conventional format.

---

## Task 2: Auth sub-tab and folder OAuth2 token store

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/stores/folder-auth-store.ts`
- Modify: `src/lib/folder-settings-convert.ts` (add the auth conversions)
- Modify: `src/components/request/AuthEditor.tsx` (new optional `inheritMessage` prop)
- Modify: `src/components/collections/folder-settings/AuthSection.tsx` (replaces the plan 08 placeholder)
- Test: `src/stores/__tests__/folder-auth-store.test.ts`
- Test: `src/lib/__tests__/folder-settings-convert.test.ts` (extend)
- Test: `src/components/request/__tests__/AuthEditor.test.tsx` (extend)
- Test: `src/components/collections/folder-settings/__tests__/AuthSection.test.tsx`

**Interfaces:**
- Consumes:
  - From Task 1: `folderChainPath`, `useFolderVariableContext`.
  - `fromPersistedAuth(auth, fallbackAuthType)` and `toPersistedAuth(auth)` from `@/lib/persisted-auth` (`fromPersistedAuth(null, 'inherit')` gives `{ authType: 'inherit' }`; `toPersistedAuth` maps `inherit` to `{ authType: 'inherit' }`).
  - `authStateForType(authType, prev)` from `@/lib/auth-type-defaults`.
  - `AuthTypeOption`, `NTLM_OPTION`, `OAUTH1_OPTION` from `@/lib/auth-type-options`.
  - `AuthEditor` props `{ auth, onChange, variableContext?, onNavigateToSource?, collection?, environmentName?, requestPath? }`.
  - `Auth`, `FolderSettings` from `@/lib/tauri-api`; `AuthState` from `@/types/pane-types`.
- Produces:
  - `useFolderAuthStore` with `auths: Record<string, AuthState>`, `setFolderAuth(collection: string, folderPath: string, auth: AuthState): void`, `getFolderAuth(collection: string, folderPath: string): AuthState | undefined`, `clearFolderAuth(collection: string, folderPath: string): void`; and `folderAuthKey(collection: string, folderPath: string): string`.
  - `folderAuthToState(auth: Auth | null | undefined): AuthState`, `stateToFolderAuth(state: AuthState): Auth | null`, `restoreOAuth2Tokens(disk: AuthState, cached: AuthState | undefined): AuthState`, `FOLDER_AUTH_TYPES: AuthTypeOption[]`, `folderAuthTypeOptions(current: AuthState['authType']): AuthTypeOption[]`.
  - `AuthEditor` optional prop `inheritMessage?: string`, replacing the text of the Inherit card.
  - `AuthSection(props: { collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void })`.

**OAuth2 decision (stated here so the implementer does not guess):** tokens are cached per folder, keyed by collection and folder path, in `folder-auth-store`. The folder's Auth sub-tab fetches and refreshes tokens with the same `AuthEditor` and the same `oauth2GetToken` command as the collection tab. `AuthEditor` is given `requestPath = folderChainPath(folderPath)` so `oauth2_service.rs` builds its variable context (collection < environment < folder chain < request) with this folder's own variables included. Sending a request that inherits a folder's OAuth2 uses that cached token (Task 3). Auto-fetch on send and the backend-only paths are listed in Known gaps 2 and 3.

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md` (auth section: `inherit`, `none`, OAuth2 shapes).

- [ ] **Step 2: Write the failing store test**

Create `src/stores/__tests__/folder-auth-store.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { folderAuthKey, useFolderAuthStore } from '@/stores/folder-auth-store';

describe('folder-auth-store', () => {
  beforeEach(() => useFolderAuthStore.setState({ auths: {} }));

  it('keeps auth per collection and folder', () => {
    const { setFolderAuth, getFolderAuth } = useFolderAuthStore.getState();
    setFolderAuth('demo', 'api', { authType: 'bearer', bearer: { token: 'a' } });
    setFolderAuth('demo', 'api/users', { authType: 'bearer', bearer: { token: 'b' } });
    setFolderAuth('other', 'api', { authType: 'bearer', bearer: { token: 'c' } });
    expect(getFolderAuth('demo', 'api')?.bearer?.token).toBe('a');
    expect(getFolderAuth('demo', 'api/users')?.bearer?.token).toBe('b');
    expect(getFolderAuth('other', 'api')?.bearer?.token).toBe('c');
    expect(getFolderAuth('demo', 'missing')).toBeUndefined();
  });

  it('clears one folder', () => {
    const { setFolderAuth, clearFolderAuth, getFolderAuth } = useFolderAuthStore.getState();
    setFolderAuth('demo', 'api', { authType: 'none' });
    clearFolderAuth('demo', 'api');
    expect(getFolderAuth('demo', 'api')).toBeUndefined();
  });

  it('builds a key that cannot collide across collection and path', () => {
    expect(folderAuthKey('a', 'b/c')).not.toBe(folderAuthKey('a/b', 'c'));
  });
});
```

- [ ] **Step 3: Run it and see it fail**

Run: `yarn test --run src/stores/__tests__/folder-auth-store.test.ts`
Expected: FAIL, cannot resolve `@/stores/folder-auth-store`.

- [ ] **Step 4: Implement the store**

Create `src/stores/folder-auth-store.ts`:

```ts
import { create } from 'zustand';
import type { AuthState } from '@/types/pane-types';

/** Key for one folder of one collection. A NUL separator cannot appear in either part. */
export function folderAuthKey(collection: string, folderPath: string): string {
  return `${collection}\u0000${folderPath}`;
}

// Holds the full AuthState of each folder that has authorization, including any fetched
// OAuth2 token. Tokens are never written to folder.yml, so this is the only place they live,
// like collection-auth-store for collections. In memory only: lost on reload by design.
interface FolderAuthStore {
  auths: Record<string, AuthState>;
  setFolderAuth: (collection: string, folderPath: string, auth: AuthState) => void;
  getFolderAuth: (collection: string, folderPath: string) => AuthState | undefined;
  clearFolderAuth: (collection: string, folderPath: string) => void;
}

export const useFolderAuthStore = create<FolderAuthStore>()((set, get) => ({
  auths: {},

  setFolderAuth(collection, folderPath, auth) {
    set({ auths: { ...get().auths, [folderAuthKey(collection, folderPath)]: auth } });
  },

  getFolderAuth(collection, folderPath) {
    return get().auths[folderAuthKey(collection, folderPath)];
  },

  clearFolderAuth(collection, folderPath) {
    const { [folderAuthKey(collection, folderPath)]: _removed, ...rest } = get().auths;
    set({ auths: rest });
  },
}));
```

- [ ] **Step 5: Run the store test and see it pass**

Run: `yarn test --run src/stores/__tests__/folder-auth-store.test.ts`
Expected: PASS.

- [ ] **Step 6: Write the failing auth-conversion tests**

Append to `src/lib/__tests__/folder-settings-convert.test.ts` (add the new names to the existing import from `@/lib/folder-settings-convert`, and add `import { authStateForType } from '@/lib/auth-type-defaults';` at the top):

```ts
describe('folder auth conversion', () => {
  it('maps no folder auth to Inherit and back to null', () => {
    expect(folderAuthToState(null)).toEqual({ authType: 'inherit' });
    expect(folderAuthToState(undefined)).toEqual({ authType: 'inherit' });
    expect(stateToFolderAuth({ authType: 'inherit' })).toBeNull();
  });

  it('keeps an on-disk none as none', () => {
    expect(folderAuthToState({ authType: 'none' })).toEqual({ authType: 'none' });
    expect(stateToFolderAuth({ authType: 'none' })).toEqual({ authType: 'none' });
  });

  it('round-trips bearer through the persisted shape', () => {
    const state = { authType: 'bearer', bearer: { token: 't' } } as const;
    const persisted = stateToFolderAuth(state);
    expect(persisted).toEqual({ authType: 'bearer', token: 't' });
    expect(folderAuthToState(persisted)).toEqual(state);
  });
});

describe('folderAuthTypeOptions', () => {
  it('does not offer None', () => {
    expect(folderAuthTypeOptions('basic').map((o) => o.value)).not.toContain('none');
    expect(folderAuthTypeOptions('inherit')[0]).toEqual({ label: 'Inherit', value: 'inherit' });
  });

  it('shows an existing on-disk none as a read-only first entry', () => {
    expect(folderAuthTypeOptions('none')[0]).toEqual({
      label: 'No Auth (set on disk)',
      value: 'none',
    });
  });
});

describe('restoreOAuth2Tokens', () => {
  const disk = authStateForType('oauth2', { authType: 'none' });
  const cached = {
    ...disk,
    oauth2: { ...(disk.oauth2 as NonNullable<typeof disk.oauth2>), accessToken: 'tok', refreshToken: 'ref' },
  };

  it('fills the tokens a disk copy lacks', () => {
    const out = restoreOAuth2Tokens(disk, cached);
    expect(out.oauth2?.accessToken).toBe('tok');
    expect(out.oauth2?.refreshToken).toBe('ref');
  });

  it('leaves a disk copy that already has a token alone', () => {
    const withToken = { ...disk, oauth2: { ...(disk.oauth2 as NonNullable<typeof disk.oauth2>), accessToken: 'mine' } };
    expect(restoreOAuth2Tokens(withToken, cached)).toBe(withToken);
  });

  it('leaves non-OAuth2 state and a missing cache alone', () => {
    const bearer = { authType: 'bearer', bearer: { token: 't' } } as const;
    expect(restoreOAuth2Tokens(bearer, cached)).toBe(bearer);
    expect(restoreOAuth2Tokens(disk, undefined)).toBe(disk);
  });
});
```

- [ ] **Step 7: Run it and see it fail**

Run: `yarn test --run src/lib/__tests__/folder-settings-convert.test.ts`
Expected: FAIL, the new names are not exported.

- [ ] **Step 8: Implement the auth conversions**

In `src/lib/folder-settings-convert.ts`, replace the import block at the top with:

```ts
import {
  type AuthTypeOption,
  NTLM_OPTION,
  OAUTH1_OPTION,
} from '@/lib/auth-type-options';
import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { Auth, Header } from '@/lib/tauri-api';
import type { AuthState, KeyValueEntry } from '@/types/pane-types';
```

Append to the file:

```ts
/** The editor state for a folder's persisted auth. No folder auth reads as Inherit. */
export function folderAuthToState(auth: Auth | null | undefined): AuthState {
  return fromPersistedAuth(auth, 'inherit');
}

/**
 * The persisted auth for the editor state. Inherit is stored as no folder auth, which is
 * how the backend already reads both `none` and `inherit` (`resolve_folder_auth`).
 */
export function stateToFolderAuth(state: AuthState): Auth | null {
  return state.authType === 'inherit' ? null : toPersistedAuth(state);
}

/**
 * Copies the OAuth2 token fields of an in-memory state onto the state read from disk, which
 * never holds tokens. Mirrors what CollectionOverviewTab does on load. A disk copy that
 * already has an access token is returned unchanged.
 */
export function restoreOAuth2Tokens(disk: AuthState, cached: AuthState | undefined): AuthState {
  if (disk.authType !== 'oauth2' || !disk.oauth2 || disk.oauth2.accessToken) return disk;
  if (cached?.authType !== 'oauth2' || !cached.oauth2?.accessToken) return disk;
  return {
    ...disk,
    oauth2: {
      ...disk.oauth2,
      accessToken: cached.oauth2.accessToken,
      refreshToken: cached.oauth2.refreshToken ?? '',
      expiresIn: cached.oauth2.expiresIn ?? null,
      tokenAcquiredAt: cached.oauth2.tokenAcquiredAt ?? null,
      idToken: cached.oauth2.idToken ?? '',
      idTokenClaims: cached.oauth2.idTokenClaims ?? null,
      accessTokenClaims: cached.oauth2.accessTokenClaims ?? null,
      tokenType: cached.oauth2.tokenType ?? '',
      responseScope: cached.oauth2.responseScope ?? '',
    },
  };
}

/**
 * Types a folder can be set to. There is no None: the backend skips None and Inherit alike
 * when it looks for folder auth, so a folder cannot switch auth off for its children.
 */
export const FOLDER_AUTH_TYPES: AuthTypeOption[] = [
  { label: 'Inherit', value: 'inherit' },
  { label: 'Basic', value: 'basic' },
  { label: 'Digest', value: 'digest' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  OAUTH1_OPTION,
  NTLM_OPTION,
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
  { label: 'WSSE', value: 'wsse' },
];

const LEGACY_NONE_OPTION: AuthTypeOption = { label: 'No Auth (set on disk)', value: 'none' };

/** The selector entries. An existing on-disk `none` stays visible so it keeps a label. */
export function folderAuthTypeOptions(current: AuthState['authType']): AuthTypeOption[] {
  return current === 'none' ? [LEGACY_NONE_OPTION, ...FOLDER_AUTH_TYPES] : FOLDER_AUTH_TYPES;
}
```

- [ ] **Step 9: Run the conversion tests and see them pass**

Run: `yarn test --run src/lib/__tests__/folder-settings-convert.test.ts`
Expected: PASS.

- [ ] **Step 10: Write the failing AuthEditor test for the custom message**

Append inside `src/components/request/__tests__/AuthEditor.test.tsx` (new top-level `describe` at the end of the file):

```tsx
describe('AuthEditor inherit message', () => {
  it('shows the request wording by default', () => {
    render(<AuthEditor auth={{ authType: 'inherit' }} onChange={vi.fn()} />);
    expect(screen.getByText(/This request inherits authorization/)).toBeInTheDocument();
  });

  it('shows a custom message when one is given', () => {
    render(
      <AuthEditor
        auth={{ authType: 'inherit' }}
        onChange={vi.fn()}
        inheritMessage='Inherited from the folder "api".'
      />,
    );
    expect(screen.getByText('Inherited from the folder "api".')).toBeInTheDocument();
    expect(screen.queryByText(/This request inherits authorization/)).toBeNull();
  });
});
```

- [ ] **Step 11: Run it and see it fail**

Run: `yarn test --run src/components/request/__tests__/AuthEditor.test.tsx`
Expected: FAIL on the custom message test (`inheritMessage` is not a prop yet; tsc would also flag it).

- [ ] **Step 12: Add the `inheritMessage` prop**

In `src/components/request/AuthEditor.tsx`, add the prop to the interface and the destructuring:

```tsx
  requestPath?: string;
  /** Replaces the text of the Inherit card, for callers that know where auth comes from. */
  inheritMessage?: string;
}
```

```tsx
  collection,
  environmentName,
  requestPath,
  inheritMessage,
}: AuthEditorProps) {
```

Replace the Inherit card paragraph:

```tsx
            <p className='text-xs text-muted-foreground'>
              This request inherits authorization from the collection settings. To override, select
              a different auth type above.
            </p>
```

with:

```tsx
            <p className='text-xs text-muted-foreground'>
              {inheritMessage ??
                'This request inherits authorization from the collection settings. To override, select a different auth type above.'}
            </p>
```

- [ ] **Step 13: Run the AuthEditor tests and see them pass**

Run: `yarn test --run src/components/request/__tests__/AuthEditor.test.tsx`
Expected: PASS, including the pre-existing tests.

- [ ] **Step 14: Write the failing AuthSection tests**

Create `src/components/collections/folder-settings/__tests__/AuthSection.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import { stateToFolderAuth } from '@/lib/folder-settings-convert';
import type { FolderSettings } from '@/lib/tauri-api';
import { useFolderAuthStore } from '@/stores/folder-auth-store';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));
vi.mock('@/components/request/oauth2/OAuth2AuthEditor', () => ({
  OAuth2AuthEditor: ({ oauth2 }: { oauth2: { accessToken: string } }) => (
    <div data-testid='oauth2-token'>{oauth2.accessToken}</div>
  ),
}));
vi.mock('@/hooks/useFolderVariableContext', () => ({
  useFolderVariableContext: () => ({ variableContext: new Map(), environmentName: undefined }),
}));
// Radix Select needs pointer APIs jsdom lacks, so it is replaced by a plain list of options.
vi.mock('@/components/ui/select', async () => {
  const React = await import('react');
  const Ctx = React.createContext<(v: string) => void>(() => undefined);
  return {
    Select: ({
      value,
      onValueChange,
      children,
    }: {
      value: string;
      onValueChange: (v: string) => void;
      children: React.ReactNode;
    }) => (
      <Ctx.Provider value={onValueChange}>
        <div data-testid='auth-type' data-value={value}>
          {children}
        </div>
      </Ctx.Provider>
    ),
    SelectTrigger: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
    SelectValue: () => null,
    SelectContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
    SelectItem: ({ value, children }: { value: string; children: React.ReactNode }) => {
      const pick = React.useContext(Ctx);
      return (
        <button type='button' onClick={() => pick(value)}>
          {children}
        </button>
      );
    },
  };
});

import { AuthSection } from '../AuthSection';

const settings = (auth: unknown = null): FolderSettings =>
  ({
    headers: [],
    auth,
    variables: [],
    preRequestScript: null,
    postResponseScript: null,
    testsScript: null,
    docs: null,
  }) as unknown as FolderSettings;

const renderSection = (auth: unknown = null, onChange = vi.fn()) =>
  render(
    <AuthSection collectionName='demo' folderPath='api' settings={settings(auth)} onChange={onChange} />,
  );

describe('AuthSection', () => {
  beforeEach(() => useFolderAuthStore.setState({ auths: {} }));

  it('shows Inherit and the explanatory note when the folder has no auth', () => {
    renderSection(null);
    expect(screen.getByTestId('auth-type')).toHaveAttribute('data-value', 'inherit');
    expect(screen.getByText(/cannot switch authorization off/)).toBeInTheDocument();
  });

  it('does not offer None for a folder', () => {
    renderSection(null);
    expect(screen.queryByRole('button', { name: 'None' })).toBeNull();
    expect(screen.queryByRole('button', { name: /No Auth/ })).toBeNull();
    expect(screen.getByRole('button', { name: 'Bearer' })).toBeInTheDocument();
  });

  it('keeps an on-disk none visible and does not rewrite it', () => {
    const onChange = vi.fn();
    renderSection({ authType: 'none' }, onChange);
    expect(screen.getByTestId('auth-type')).toHaveAttribute('data-value', 'none');
    expect(screen.getByRole('button', { name: 'No Auth (set on disk)' })).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it('loads an existing bearer token into the editor', () => {
    renderSection({ authType: 'bearer', token: 'abc' });
    expect(screen.getByLabelText('Bearer token')).toHaveValue('abc');
  });

  it('choosing a type sends the persisted auth with that type defaults', () => {
    const onChange = vi.fn();
    renderSection(null, onChange);
    fireEvent.click(screen.getByRole('button', { name: 'Bearer' }));
    expect(onChange).toHaveBeenLastCalledWith({ auth: { authType: 'bearer', token: '' } });
  });

  it('typing a token sends the persisted bearer auth', () => {
    const onChange = vi.fn();
    renderSection({ authType: 'bearer', token: 'a' }, onChange);
    fireEvent.change(screen.getByLabelText('Bearer token'), { target: { value: 'abc' } });
    expect(onChange).toHaveBeenLastCalledWith({ auth: { authType: 'bearer', token: 'abc' } });
  });

  it('choosing Inherit clears the folder auth', () => {
    const onChange = vi.fn();
    renderSection({ authType: 'bearer', token: 'a' }, onChange);
    fireEvent.click(screen.getByRole('button', { name: 'Inherit' }));
    expect(onChange).toHaveBeenLastCalledWith({ auth: null });
  });

  it('restores a cached OAuth2 token into the editor', () => {
    const state = authStateForType('oauth2', { authType: 'none' });
    useFolderAuthStore.getState().setFolderAuth('demo', 'api', {
      ...state,
      oauth2: { ...(state.oauth2 as NonNullable<typeof state.oauth2>), accessToken: 'cached' },
    });
    renderSection(stateToFolderAuth(state));
    expect(screen.getByTestId('oauth2-token')).toHaveTextContent('cached');
  });

  it('writes every edit to the folder auth store', () => {
    renderSection(null);
    fireEvent.click(screen.getByRole('button', { name: 'OAuth 2.0' }));
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')?.authType).toBe('oauth2');
  });

  it('resets the editor when settings.auth changes from outside', () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <AuthSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ authType: 'bearer', token: 'a' })}
        onChange={onChange}
      />,
    );
    rerender(
      <AuthSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ authType: 'bearer', token: 'z' })}
        onChange={onChange}
      />,
    );
    expect(screen.getByLabelText('Bearer token')).toHaveValue('z');
  });
});
```

- [ ] **Step 15: Run it and see it fail**

Run: `yarn test --run src/components/collections/folder-settings/__tests__/AuthSection.test.tsx`
Expected: FAIL (the placeholder renders no selector or editor).

- [ ] **Step 16: Implement AuthSection**

Replace the contents of `src/components/collections/folder-settings/AuthSection.tsx` (keep the export style found in Task 1, Step 2):

```tsx
import { ShieldCheck } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AuthEditor } from '@/components/request/AuthEditor';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useFolderVariableContext } from '@/hooks/useFolderVariableContext';
import { authStateForType } from '@/lib/auth-type-defaults';
import {
  folderAuthToState,
  folderAuthTypeOptions,
  folderChainPath,
  restoreOAuth2Tokens,
  stateToFolderAuth,
} from '@/lib/folder-settings-convert';
import type { FolderSettings } from '@/lib/tauri-api';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import type { AuthState } from '@/types/pane-types';

interface AuthSectionProps {
  collectionName: string;
  folderPath: string;
  settings: FolderSettings;
  onChange: (patch: Partial<FolderSettings>) => void;
}

const INHERIT_NOTE =
  'No authorization is set on this folder. Requests below it that are set to Inherit use the nearest parent folder with authorization, then the collection. A folder cannot switch authorization off for the requests below it.';

const NONE_ON_DISK_NOTE =
  'This folder has "No Auth" set in its folder.yml. It has no effect: requests below it still inherit from the parent folder or the collection. Choose Inherit to remove it.';

function loadAuthState(
  collectionName: string,
  folderPath: string,
  persisted: FolderSettings['auth'],
): AuthState {
  return restoreOAuth2Tokens(
    folderAuthToState(persisted),
    useFolderAuthStore.getState().getFolderAuth(collectionName, folderPath),
  );
}

export function AuthSection({ collectionName, folderPath, settings, onChange }: AuthSectionProps) {
  // The editor state lives here because OAuth2 tokens are not part of the persisted shape.
  const [auth, setAuth] = useState<AuthState>(() =>
    loadAuthState(collectionName, folderPath, settings.auth),
  );
  const { variableContext, environmentName } = useFolderVariableContext(
    collectionName,
    folderPath,
    settings.variables,
  );

  // Refs so the edit handler can write to the token store even when this component has
  // unmounted, for example when the user switches tabs during an OAuth2 browser flow.
  const targetRef = useRef({ collectionName, folderPath });
  targetRef.current = { collectionName, folderPath };
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // biome-ignore lint/correctness/useExhaustiveDependencies: only an outside change to settings.auth or a new folder resets the editor; the local state is compared inside.
  useEffect(() => {
    if (JSON.stringify(settings.auth ?? null) !== JSON.stringify(stateToFolderAuth(auth))) {
      setAuth(loadAuthState(collectionName, folderPath, settings.auth));
    }
  }, [settings.auth, collectionName, folderPath]);

  const commit = useCallback((next: AuthState) => {
    setAuth(next);
    const target = targetRef.current;
    useFolderAuthStore.getState().setFolderAuth(target.collectionName, target.folderPath, next);
    onChangeRef.current({ auth: stateToFolderAuth(next) });
  }, []);

  const handleTypeChange = useCallback(
    (authType: AuthState['authType']) => commit(authStateForType(authType, auth)),
    [auth, commit],
  );

  return (
    <div className='p-4 max-w-2xl'>
      <Card>
        <CardHeader className='pb-3 pt-4 px-4 border-b border-border/40'>
          <div className='flex items-center justify-between'>
            <div className='flex items-center gap-2'>
              <ShieldCheck className='h-4 w-4 text-muted-foreground' />
              <CardTitle className='text-sm font-medium'>Authorization</CardTitle>
            </div>
            <div className='flex items-center gap-2'>
              <span className='text-xs text-muted-foreground shrink-0'>Auth type</span>
              <Select
                value={auth.authType}
                onValueChange={(v) => handleTypeChange(v as AuthState['authType'])}
              >
                <SelectTrigger aria-label='Auth type' className='h-7 w-40 text-xs'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {folderAuthTypeOptions(auth.authType).map((t) => (
                    <SelectItem key={t.value} value={t.value} className='text-sm'>
                      {t.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
        </CardHeader>
        <CardContent className='p-4 space-y-4'>
          {auth.authType === 'none' && (
            <Card className='bg-muted/50'>
              <CardContent className='px-3 py-2.5'>
                <p className='text-xs text-muted-foreground'>{NONE_ON_DISK_NOTE}</p>
              </CardContent>
            </Card>
          )}
          <AuthEditor
            auth={auth}
            onChange={commit}
            variableContext={variableContext}
            collection={collectionName}
            environmentName={environmentName}
            requestPath={folderChainPath(folderPath) || undefined}
            inheritMessage={INHERIT_NOTE}
          />
        </CardContent>
      </Card>
    </div>
  );
}
```

Note: the `none` case shows both the on-disk note and the `AuthEditor` line "No authentication configured.", which is acceptable. The `SelectItem` `className` prop is accepted by the real component and ignored by the test stub.

- [ ] **Step 17: Run the AuthSection tests and see them pass**

Run: `yarn test --run src/components/collections/folder-settings/__tests__/AuthSection.test.tsx`
Expected: PASS (10 tests).

- [ ] **Step 18: Type-check and lint**

Run: `yarn tsc --noEmit` then `yarn check`.
Expected: both clean (apply `yarn lint` for import order or format only, then re-check).

- [ ] **Step 19: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only, then pathspec-commit:

```bash
git add src/stores/folder-auth-store.ts src/lib/folder-settings-convert.ts src/components/request/AuthEditor.tsx src/components/collections/folder-settings/AuthSection.tsx src/stores/__tests__/folder-auth-store.test.ts src/lib/__tests__/folder-settings-convert.test.ts src/components/request/__tests__/AuthEditor.test.tsx src/components/collections/folder-settings/__tests__/AuthSection.test.tsx
git commit --only -m "feat(folder-settings): add the Auth sub-tab" -- src/stores/folder-auth-store.ts src/lib/folder-settings-convert.ts src/components/request/AuthEditor.tsx src/components/collections/folder-settings/AuthSection.tsx src/stores/__tests__/folder-auth-store.test.ts src/lib/__tests__/folder-settings-convert.test.ts src/components/request/__tests__/AuthEditor.test.tsx src/components/collections/folder-settings/__tests__/AuthSection.test.tsx
```

---

## Task 3: Inherited auth resolution, request-side hint and send path

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

The hint is cheap with existing data, so it is included: it needs only `getFolderSettings` per ancestor folder (one small IPC call each, only while the Auth tab shows Inherit) plus the collection settings. The same resolver fixes a real conflict in the send path. Today `resolveRequestFieldsForPath` replaces an inheriting request's auth with the collection's cached auth before the backend ever sees `inherit`, so a folder's auth (plan 05) would never apply whenever the collection has auth. Task 3 makes the folder win there, as the spec's Auth rule requires.

**Files:**
- Create: `src/lib/inherited-auth.ts`
- Create: `src/hooks/useInheritedAuthSource.ts`
- Modify: `src/lib/execute-request.ts` (the `authToResolve` block inside `resolveRequestFieldsForPath`)
- Modify: `src/components/request/RequestPanel.tsx` (hook call and `inheritMessage` on the Auth tab)
- Test: `src/lib/__tests__/inherited-auth.test.ts`
- Test: `src/hooks/__tests__/useInheritedAuthSource.test.ts`
- Test: `src/lib/__tests__/execute-request.folder-auth.test.ts`

**Interfaces:**
- Consumes:
  - From Task 2: `folderAuthToState`, `restoreOAuth2Tokens`, `FOLDER_AUTH_TYPES`, `useFolderAuthStore`.
  - `getFolderSettings(collection, folderPath)`, `getCollectionSettings(collection)` from `@/lib/tauri-api`; `fromPersistedAuth` from `@/lib/persisted-auth`; `useCollectionAuthStore` from `@/stores/collection-auth-store`.
- Produces:
  - `ancestorFolderPaths(requestPath: string): string[]` (outermost first, root excluded).
  - `resolveInheritedFolderAuth(collection: string, requestPath: string): Promise<{ folderPath: string; auth: AuthState } | undefined>` (innermost folder with auth other than none and inherit; never rejects).
  - `type InheritedAuthSource = { kind: 'folder'; folderPath: string; auth: AuthState } | { kind: 'collection'; auth: AuthState } | { kind: 'none' }`
  - `resolveInheritedAuthSource(collection: string, requestPath: string): Promise<InheritedAuthSource>` (never rejects).
  - `describeInheritedAuthSource(source: InheritedAuthSource): string`.
  - `useInheritedAuthSource(collection: string | undefined, requestPath: string | undefined, enabled: boolean): InheritedAuthSource | undefined`.

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md` (auth `inherit` semantics).

- [ ] **Step 2: Write the failing resolver tests**

Create `src/lib/__tests__/inherited-auth.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import { stateToFolderAuth } from '@/lib/folder-settings-convert';

const api = vi.hoisted(() => ({
  getFolderSettings: vi.fn(),
  getCollectionSettings: vi.fn(),
}));
vi.mock('@/lib/tauri-api', () => api);

import {
  ancestorFolderPaths,
  describeInheritedAuthSource,
  resolveInheritedFolderAuth,
  resolveInheritedAuthSource,
} from '@/lib/inherited-auth';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';

const folders: Record<string, unknown> = {};
const basic = { authType: 'basic', username: 'u', password: 'p' };

describe('ancestorFolderPaths', () => {
  it('lists folders outermost first and excludes the root', () => {
    expect(ancestorFolderPaths('a/b/req.yml')).toEqual(['a', 'a/b']);
    expect(ancestorFolderPaths('a/req.yml')).toEqual(['a']);
    expect(ancestorFolderPaths('req.yml')).toEqual([]);
  });
});

describe('resolveInheritedAuthSource', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const k of Object.keys(folders)) delete folders[k];
    useFolderAuthStore.setState({ auths: {} });
    useCollectionAuthStore.setState({ auths: new Map() });
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
      auth: folders[path] ?? null,
    }));
    api.getCollectionSettings.mockResolvedValue({ auth: null });
  });

  it('uses the nearest folder with auth', async () => {
    folders.a = basic;
    folders['a/b'] = { authType: 'bearer', token: 't' };
    const out = await resolveInheritedAuthSource('demo', 'a/b/req.yml');
    expect(out).toMatchObject({ kind: 'folder', folderPath: 'a/b' });
  });

  it('skips folders that are Inherit or None', async () => {
    folders.a = basic;
    folders['a/b'] = { authType: 'none' };
    const out = await resolveInheritedAuthSource('demo', 'a/b/c/req.yml');
    expect(out).toMatchObject({ kind: 'folder', folderPath: 'a' });
  });

  it('falls back to the collection auth store, then to the collection on disk', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'x' },
    });
    expect((await resolveInheritedAuthSource('demo', 'a/req.yml')).kind).toBe('collection');

    useCollectionAuthStore.setState({ auths: new Map() });
    api.getCollectionSettings.mockResolvedValue({ auth: basic });
    expect((await resolveInheritedAuthSource('demo', 'a/req.yml')).kind).toBe('collection');
  });

  it('reports none when nothing sets auth', async () => {
    expect(await resolveInheritedAuthSource('demo', 'a/req.yml')).toEqual({ kind: 'none' });
  });

  it('skips a folder whose settings cannot be read', async () => {
    folders.a = basic;
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => {
      if (path === 'a/b') throw new Error('bad yaml');
      return { auth: folders[path] ?? null };
    });
    const out = await resolveInheritedAuthSource('demo', 'a/b/req.yml');
    expect(out).toMatchObject({ kind: 'folder', folderPath: 'a' });
  });

  it('puts a cached folder OAuth2 token on the folder auth', async () => {
    const oauth = authStateForType('oauth2', { authType: 'none' });
    folders.a = stateToFolderAuth(oauth);
    useFolderAuthStore.getState().setFolderAuth('demo', 'a', {
      ...oauth,
      oauth2: { ...(oauth.oauth2 as NonNullable<typeof oauth.oauth2>), accessToken: 'cached' },
    });
    const out = await resolveInheritedFolderAuth('demo', 'a/req.yml');
    expect(out?.auth.oauth2?.accessToken).toBe('cached');
  });
});

describe('describeInheritedAuthSource', () => {
  it('names the folder, the collection or nothing', () => {
    expect(
      describeInheritedAuthSource({
        kind: 'folder',
        folderPath: 'api/users',
        auth: { authType: 'bearer', bearer: { token: '' } },
      }),
    ).toBe('This request inherits authorization from the folder "api/users" (Bearer).');
    expect(
      describeInheritedAuthSource({ kind: 'collection', auth: { authType: 'basic' } }),
    ).toBe('This request inherits authorization from the collection settings (Basic).');
    expect(describeInheritedAuthSource({ kind: 'none' })).toBe(
      'No folder or collection sets authorization, so this request is sent without it.',
    );
  });
});
```

- [ ] **Step 3: Run it and see it fail**

Run: `yarn test --run src/lib/__tests__/inherited-auth.test.ts`
Expected: FAIL, cannot resolve `@/lib/inherited-auth`.

- [ ] **Step 4: Implement the resolver**

Create `src/lib/inherited-auth.ts`:

```ts
// Where a request set to Inherit gets its authorization: the nearest folder with auth, else
// the collection, else nothing. Used by the Auth tab hint and by execute-request.ts.
import { FOLDER_AUTH_TYPES, folderAuthToState, restoreOAuth2Tokens } from '@/lib/folder-settings-convert';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import { getCollectionSettings, getFolderSettings } from '@/lib/tauri-api';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import type { AuthState } from '@/types/pane-types';

export type InheritedAuthSource =
  | { kind: 'folder'; folderPath: string; auth: AuthState }
  | { kind: 'collection'; auth: AuthState }
  | { kind: 'none' };

const hasAuth = (auth: AuthState) => auth.authType !== 'none' && auth.authType !== 'inherit';

/** Folders above a request, outermost first. The collection root is not a folder here. */
export function ancestorFolderPaths(requestPath: string): string[] {
  const folders = requestPath.split('/').slice(0, -1);
  return folders.map((_, i) => folders.slice(0, i + 1).join('/'));
}

/**
 * The innermost ancestor folder whose auth is neither None nor Inherit, matching the backend
 * `resolve_folder_auth`. The folder's OAuth2 token is filled in from `folder-auth-store`.
 * A folder that cannot be read is skipped: the backend reports a broken folder.yml when the
 * request runs, and a hint must not fail on it.
 */
export async function resolveInheritedFolderAuth(
  collection: string,
  requestPath: string,
): Promise<{ folderPath: string; auth: AuthState } | undefined> {
  for (const folderPath of ancestorFolderPaths(requestPath).reverse()) {
    try {
      const settings = await getFolderSettings(collection, folderPath);
      const disk = folderAuthToState(settings.auth);
      if (!hasAuth(disk)) continue;
      const cached = useFolderAuthStore.getState().getFolderAuth(collection, folderPath);
      return { folderPath, auth: restoreOAuth2Tokens(disk, cached) };
    } catch {
      // Unreadable folder settings are skipped here on purpose.
    }
  }
  return undefined;
}

export async function resolveInheritedAuthSource(
  collection: string,
  requestPath: string,
): Promise<InheritedAuthSource> {
  const folder = await resolveInheritedFolderAuth(collection, requestPath);
  if (folder) return { kind: 'folder', ...folder };

  const cached = useCollectionAuthStore.getState().getCollectionAuth(collection);
  if (cached && hasAuth(cached)) return { kind: 'collection', auth: cached };
  try {
    const disk = fromPersistedAuth((await getCollectionSettings(collection)).auth);
    if (hasAuth(disk)) return { kind: 'collection', auth: disk };
  } catch {
    // Collection settings are unavailable: treat the collection as having no auth.
  }
  return { kind: 'none' };
}

const typeLabel = (auth: AuthState) =>
  FOLDER_AUTH_TYPES.find((t) => t.value === auth.authType)?.label ?? auth.authType;

export function describeInheritedAuthSource(source: InheritedAuthSource): string {
  switch (source.kind) {
    case 'folder':
      return `This request inherits authorization from the folder "${source.folderPath}" (${typeLabel(source.auth)}).`;
    case 'collection':
      return `This request inherits authorization from the collection settings (${typeLabel(source.auth)}).`;
    case 'none':
      return 'No folder or collection sets authorization, so this request is sent without it.';
  }
}
```

- [ ] **Step 5: Run the resolver tests and see them pass**

Run: `yarn test --run src/lib/__tests__/inherited-auth.test.ts`
Expected: PASS. If `getCollectionSettings(...).auth` fails to type-check in `tsc`, mirror the cast `CollectionOverviewTab` uses (`fromPersistedAuth(s.auth)` where `s` is the collection settings).

- [ ] **Step 6: Write the failing hook test**

Create `src/hooks/__tests__/useInheritedAuthSource.test.ts`:

```ts
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const resolve = vi.hoisted(() => vi.fn());
vi.mock('@/lib/inherited-auth', () => ({ resolveInheritedAuthSource: resolve }));

import { useInheritedAuthSource } from '../useInheritedAuthSource';

describe('useInheritedAuthSource', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resolve.mockResolvedValue({ kind: 'none' });
  });

  it('resolves the source when enabled', async () => {
    const { result } = renderHook(() => useInheritedAuthSource('demo', 'a/req.yml', true));
    await waitFor(() => expect(result.current).toEqual({ kind: 'none' }));
    expect(resolve).toHaveBeenCalledWith('demo', 'a/req.yml');
  });

  it('does nothing when disabled or when the request has no source', () => {
    const off = renderHook(() => useInheritedAuthSource('demo', 'a/req.yml', false));
    const none = renderHook(() => useInheritedAuthSource(undefined, undefined, true));
    expect(off.result.current).toBeUndefined();
    expect(none.result.current).toBeUndefined();
    expect(resolve).not.toHaveBeenCalled();
  });

  it('drops the source again when it becomes disabled', async () => {
    const { result, rerender } = renderHook(
      ({ enabled }) => useInheritedAuthSource('demo', 'a/req.yml', enabled),
      { initialProps: { enabled: true } },
    );
    await waitFor(() => expect(result.current).toBeDefined());
    rerender({ enabled: false });
    await waitFor(() => expect(result.current).toBeUndefined());
  });
});
```

- [ ] **Step 7: Run it and see it fail**

Run: `yarn test --run src/hooks/__tests__/useInheritedAuthSource.test.ts`
Expected: FAIL, cannot resolve `../useInheritedAuthSource`.

- [ ] **Step 8: Implement the hook**

Create `src/hooks/useInheritedAuthSource.ts`:

```ts
import { useEffect, useState } from 'react';
import { type InheritedAuthSource, resolveInheritedAuthSource } from '@/lib/inherited-auth';

/**
 * Where a request's inherited authorization comes from. Resolves only while `enabled`, so it
 * costs nothing unless the Auth tab is open on a request set to Inherit.
 */
export function useInheritedAuthSource(
  collection: string | undefined,
  requestPath: string | undefined,
  enabled: boolean,
): InheritedAuthSource | undefined {
  const [source, setSource] = useState<InheritedAuthSource>();

  useEffect(() => {
    if (!enabled || !collection || !requestPath) {
      setSource(undefined);
      return;
    }
    let cancelled = false;
    void resolveInheritedAuthSource(collection, requestPath).then((s) => {
      if (!cancelled) setSource(s);
    });
    return () => {
      cancelled = true;
    };
  }, [collection, requestPath, enabled]);

  return source;
}
```

- [ ] **Step 9: Run the hook test and see it pass**

Run: `yarn test --run src/hooks/__tests__/useInheritedAuthSource.test.ts`
Expected: PASS.

- [ ] **Step 10: Write the failing send-path test**

Create `src/lib/__tests__/execute-request.folder-auth.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import { stateToFolderAuth } from '@/lib/folder-settings-convert';
import type { RequestState } from '@/types/pane-types';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
  getFolderChainVariables: vi.fn(),
  getRequestVariables: vi.fn(),
  getFolderSettings: vi.fn(),
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));
vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({ getQueryData: () => undefined }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';

const request = (authType: 'inherit' | 'none'): RequestState => ({
  requestType: 'http',
  method: 'GET',
  url: 'https://api.example/ping',
  pathParams: [],
  queryParams: [],
  headers: [],
  body: { mode: 'none', content: '', formData: [] },
  auth: { authType },
  settings: {
    verifySsl: true,
    followRedirects: true,
    maxRedirects: 5,
    timeoutMs: 0,
    encodeUrl: true,
  },
  docs: null,
  tags: [],
  assertions: [],
  actions: [],
});

describe('resolveRequestFieldsForPath inherited folder auth', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useFolderAuthStore.setState({ auths: {} });
    useCollectionAuthStore.setState({ auths: new Map() });
    api.getCollectionSettings.mockResolvedValue({ variables: [], headers: [], auth: null });
    api.getFolderChainVariables.mockResolvedValue([]);
    api.getRequestVariables.mockResolvedValue([]);
    api.getFolderSettings.mockResolvedValue({ auth: null });
  });

  it('uses the nearest folder auth for an inheriting request', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
      auth: path === 'api' ? { authType: 'basic', username: 'u', password: 'p' } : null,
    }));
    const out = await resolveRequestFieldsForPath('demo', 'api/users/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'basic', username: 'u', password: 'p' });
  });

  it('a folder OAuth2 token reaches the wire as a bearer token', async () => {
    const oauth = authStateForType('oauth2', { authType: 'none' });
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
      auth: path === 'api' ? stateToFolderAuth(oauth) : null,
    }));
    useFolderAuthStore.getState().setFolderAuth('demo', 'api', {
      ...oauth,
      oauth2: { ...(oauth.oauth2 as NonNullable<typeof oauth.oauth2>), accessToken: 'folder-token' },
    });
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'bearer', token: 'folder-token' });
  });

  it('keeps inherit on the wire when no folder sets auth and the collection has none cached', async () => {
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'inherit' });
  });

  it('still uses the cached collection auth when no folder sets auth', async () => {
    useCollectionAuthStore.getState().setCollectionAuth('demo', {
      authType: 'bearer',
      bearer: { token: 'collection-token' },
    });
    const out = await resolveRequestFieldsForPath('demo', 'api/get.yml', request('inherit'));
    expect(out.auth).toEqual({ authType: 'bearer', token: 'collection-token' });
  });

  it('never looks at folders for a request with its own auth', async () => {
    await resolveRequestFieldsForPath('demo', 'api/get.yml', request('none'));
    expect(api.getFolderSettings).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 11: Run it and see it fail**

Run: `yarn test --run src/lib/__tests__/execute-request.folder-auth.test.ts`
Expected: FAIL on the first two tests (the collection token wins, and the folder is never consulted).

- [ ] **Step 12: Use the folder auth in `execute-request.ts`**

In `src/lib/execute-request.ts`:

Plan 05 (step 8) has already rewritten this block. It imports `resolveFolderAuth` and `fromPersistedAuth` from `@/lib/folder-inheritance` and `@/lib/persisted-auth`, and it leaves an OAuth2 folder auth as `inherit`. This step builds on that version and keeps plan 05's header handling untouched. Replace plan 05's block:

```ts
  let authToResolve: AuthState = request.auth;
  if (request.auth.authType === 'inherit' && collection) {
    const folderAuth = resolveFolderAuth(folderChain);
    if (folderAuth) {
      // An OAuth2 folder auth stays `inherit`. The backend resolves it, and fetches a
      // client-credentials token at send time, exactly as on every other send path.
      const folderState = fromPersistedAuth(folderAuth);
      if (folderState.authType !== 'oauth2') authToResolve = folderState;
    } else {
      const storedAuth = useCollectionAuthStore.getState().getCollectionAuth(collection);
      if (storedAuth && storedAuth.authType !== 'none' && storedAuth.authType !== 'inherit') {
        authToResolve = storedAuth;
      }
    }
  }
```

with:

```ts
  let authToResolve: AuthState = request.auth;
  if (request.auth.authType === 'inherit' && collection) {
    // The nearest folder with auth wins over the collection. A folder OAuth2 token only
    // exists in the frontend store, so the folder is resolved here, with its cached token.
    const folderAuth = requestPath
      ? await resolveInheritedFolderAuth(collection, requestPath)
      : undefined;
    if (folderAuth) {
      authToResolve = folderAuth.auth;
    } else {
      const storedAuth = useCollectionAuthStore.getState().getCollectionAuth(collection);
      if (storedAuth && storedAuth.authType !== 'none' && storedAuth.authType !== 'inherit') {
        authToResolve = storedAuth;
      }
    }
  }
```

Then fix the imports at the top of `execute-request.ts`: remove `resolveFolderAuth` from the `@/lib/folder-inheritance` import and remove the `fromPersistedAuth` import if nothing else in the file uses it (Biome flags unused imports), and add, in sorted position:

```ts
import { resolveInheritedFolderAuth } from '@/lib/inherited-auth';
```

The folder chain is read twice on this path (plan 05's `loadFolderChain` for headers, this call for auth). That is a few small IPC calls per send and is accepted to keep the two plans independent.

`resolveInheritedFolderAuth` never rejects, so existing tests whose `@/lib/tauri-api` mock has no `getFolderSettings` still pass (the missing export throws inside its `try`).

- [ ] **Step 13: Run the send-path tests and the existing execute-request tests**

Run: `yarn test --run src/lib/__tests__/execute-request.folder-auth.test.ts src/lib/__tests__/execute-request.test.ts src/lib/__tests__/execute-request.oauth.test.ts src/lib/__tests__/execute-request.collection.test.ts`
Expected: PASS for all four files.

- [ ] **Step 14: Show the hint in the request Auth tab**

In `src/components/request/RequestPanel.tsx`:

Add imports in sorted position:

```tsx
import { useInheritedAuthSource } from '@/hooks/useInheritedAuthSource';
import { describeInheritedAuthSource } from '@/lib/inherited-auth';
```

Directly after the `authTypeOptions` `useMemo` (the block that starts `const currentAuthType = request.auth.authType;`), add:

```tsx
  const inheritedAuthSource = useInheritedAuthSource(
    tab.source?.collection,
    tab.source?.path,
    activeSection === 'auth' && request.auth.authType === 'inherit',
  );
```

On the `<AuthEditor` in the `activeSection === 'auth'` block, add one prop:

```tsx
            requestPath={tab.source?.path}
            inheritMessage={
              inheritedAuthSource ? describeInheritedAuthSource(inheritedAuthSource) : undefined
            }
```

`RequestPanel` is not rendered by any existing test, so this wiring is covered by the resolver and hook tests plus `yarn tsc --noEmit`.

- [ ] **Step 15: Type-check and lint**

Run: `yarn tsc --noEmit` then `yarn check`.
Expected: both clean (apply `yarn lint` for import order or format only, then re-check).

- [ ] **Step 16: Run the whole touched test set once**

Run: `yarn test --run src/lib/__tests__ src/hooks/__tests__ src/stores/__tests__ src/components/request/__tests__/AuthEditor.test.tsx src/components/collections/folder-settings`
Expected: PASS.

- [ ] **Step 17: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only, then pathspec-commit:

```bash
git add src/lib/inherited-auth.ts src/hooks/useInheritedAuthSource.ts src/lib/execute-request.ts src/components/request/RequestPanel.tsx src/lib/__tests__/inherited-auth.test.ts src/hooks/__tests__/useInheritedAuthSource.test.ts src/lib/__tests__/execute-request.folder-auth.test.ts
git commit --only -m "feat(folder-settings): resolve inherited auth through folders" -- src/lib/inherited-auth.ts src/hooks/useInheritedAuthSource.ts src/lib/execute-request.ts src/components/request/RequestPanel.tsx src/lib/__tests__/inherited-auth.test.ts src/hooks/__tests__/useInheritedAuthSource.test.ts src/lib/__tests__/execute-request.folder-auth.test.ts
```

---

## Next Plan

**Execution order:** this is plan 10 of 12. The series runs strictly one plan at a time, in numeric order (see [00-plan-index.md](00-plan-index.md)).

**Next to execute:** [Plan 11: Script and Test sub-tabs](2026-10-07-folder-settings-plan-11-script-test-sections.md).

**Instruction for the executing Claude:** when every task in this plan is done and its verification commands (listed in the last step of each task) pass, record "Task N: complete" for each task in the ledger at `.superpowers/sdd/folder-settings/progress.md`, then open Plan 11 and start its first task immediately. Do not ask the user for consent between plans. Stop and report to the user instead if a verification command fails, if a plan's stated prerequisite is missing, or if you must deviate from the locked contract in the index.

**Notes for the next plan (from the plan author):**

`docs/superpowers/plans/folder-settings/2026-10-07-folder-settings-plan-11-script-test-sections.md` (Script and Test sub-tabs). Chain to it once this plan is complete and its checks are green. The Known gaps above are to be carried into plan 12's verification list.
