# Secret Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop secret environment values from reaching the variable popover and hover unmasked (F-57), warn when an Auth node holds a literal credential (F-47 warning), and extract the shared hook and helpers that plans P13, P14 and P15 build on.

**Architecture:** `buildScopedContext` gains two optional "secret key" sets, one for the environment layer and one for the global layer, and all eight callers pass them. A new pure module `src/lib/flow-secrets.ts` knows which persisted `Auth` fields are credentials. A new hook `useCollectionVariableContext(collection)` holds the variable-scope code that today lives inline in `AuthNodeEditor`, and `AuthNodeEditor` is refactored onto it and shows the plaintext warning. Frontend only.

**Tech Stack:** React, TypeScript, Vitest and Testing Library, `@tanstack/react-query` (existing queries), Zustand (`env-store`).

**Spec:** Roadmap items F-57, F-47 (warning half) and the F-43 prerequisite in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P3). Flow auth spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md` (lines 241-244, the plaintext warning).

## What was verified before writing (F-57)

The leak is real. Read on 2026-10-08 at HEAD b047bbc6:

- `FsEnvironmentRepo::hydrate_secrets` (`crates/rocket-infra/src/fs_environment_repo.rs:83-103`) writes the real value from the secret store into `Variable.value`, and `list` (`:105-130`) and `get` (`:132-147`) both call it. The existing Rust tests `secret_value_roundtrips_through_save_and_get` (`:500`) and `secret_value_roundtrips_through_save_and_list` (`:522`) already document this, so no Rust change and no new Rust test is needed.
- The IPC commands `list_environments` and `get_environment` (`src-tauri/src/commands/environments.rs:38-57`) return the domain `Environment` as is. `Variable` serializes `value` unconditionally (`crates/rocket-environment/src/variable.rs:5-20`). So `Variable.value` on the frontend (`src/lib/tauri-api.ts:340-345`) holds the real secret whenever the secret store is reachable.
- `buildScopedContext` (`src/lib/url-variables.ts:46-87`) calls `add(k, v, 'environment', label)` and `add(k, v, 'global', 'Global')` with the default `secret = false`. Only collection, folder and request variables pass `v.secret`.
- All eight callers build `envVars: Record<string, string>` from `v.value` and drop `v.secret`: `RequestPanel.tsx:538-541`, `CollectionOverviewTab.tsx:147`, `WorkspaceEnvironmentsTab.tsx:134`, `EnvironmentDialog.tsx:412`, `useGrpcVariableContext.ts:84`, `useWebSocketVariableContext.ts:55`, `useFolderVariableContext.ts:69`, `AuthNodeEditor.tsx:115`.
- `VariablePopover.tsx:127-130` shows the real value in the input when `entry.secret` is false. The autocomplete info (`variable-autocomplete.ts:78`) and the Monaco hover (`MonacoWrapper.tsx:249-255`) already mask when `entry.secret` is true, so the only fix needed is to set the flag. The masking code is correct.
- Constraint that shapes the fix: `OAuth2AuthEditor.tsx:77-78` reads `e.value` from the context to resolve `{{vars}}` before "Get New Access Token". So a secret entry must keep its real `value` in memory and only carry `secret: true`. Blanking the value would break OAuth2 sign-in with a secret environment client secret.

Correction to the design notes: `plaintextSecretFields` must take the persisted `Auth` (flat, tagged union from `src/lib/tauri-api.ts:48-59`), not `AuthState`. `AuthNodeEditor` holds the persisted auth in `kind.auth`, and that is what is written to the flow file. The persisted names differ from the notes: OAuth 1.0 uses `accessTokenSecret` (not `tokenSecret`), OAuth 2.0 uses `credentials.clientSecret` and `resourceOwner.password` (`crates/rocket-shared/src/oauth2.rs:5-17`), and the OAuth 1.0 private key is `privateKey: { type, value }` (`crates/rocket-shared/src/types.rs:300-327`).

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No Rust changes in this plan.
- A secret value must never be placed in the DOM, a `title` attribute, a `data-*` attribute or a log line. The warning names fields, never values.
- Before starting Task 2 and Task 3, read `docs/superpowers/specs/opencollection-spec-reference.md` (they touch variable resolution and auth).
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <pattern>` listed in the task. `yarn check` flags import order; fix with `yarn lint`.
- Only one implementer at a time touches `AuthNodeEditor.tsx`. Plan P14 also edits it, so run P14 after this plan merges.
- Not in scope: masking secret values in the environment editor table itself, any change to what the backend returns over IPC, vault (`RocketVault`) lookups.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first. **Task 2 touches an invariant (secret values must not reach UI surfaces): review it in the main loop, not in a subagent.**

1. A caller of `buildScopedContext` is missed, so the leak stays open on one surface. Pinned by the caller audit test in Task 2, which fails when any file that calls `buildScopedContext` lacks `envSecretKeys`.
2. The secret entry loses its real `value`, which breaks OAuth2 variable resolution. Pinned in Task 2 (`keeps the real value on a secret entry`).
3. The flag leaks across layers: a higher-priority non-secret variable must not inherit `secret: true` from a lower layer, and a disabled secret variable must produce no entry. Pinned in Task 2.
4. The popover still renders the real value for a secret entry (the surface the user sees). Pinned in Task 2 with a DOM check on the whole document.
5. The plaintext warning flags a whole `{{variable}}` reference, flags an empty field, misses the OAuth 1.0 private key, or puts the secret value in the DOM. Pinned in Tasks 1 and 3.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/flow-secrets.ts` (new) | `isVariableReference`, `plaintextSecretFields`, `redactPlaintextSecrets`. Pure, no I/O. |
| `src/lib/url-variables.ts` (modify) | `secretKeysOf`, and the `envSecretKeys` and `globalSecretKeys` parameters of `buildScopedContext`. |
| `src/hooks/useCollectionVariableContext.ts` (new) | `useCollectionVariableContext(collection)`: the variable scope of one collection for flow editors. |
| `src/components/flow/properties/AuthNodeEditor.tsx` (modify) | Uses the hook, shows the plaintext warning. |
| Seven other callers (modify) | `RequestPanel.tsx`, `CollectionOverviewTab.tsx`, `WorkspaceEnvironmentsTab.tsx`, `EnvironmentDialog.tsx`, `useGrpcVariableContext.ts`, `useWebSocketVariableContext.ts`, `useFolderVariableContext.ts` pass the new sets. |

Interfaces other plans consume (names and signatures are fixed here):

```ts
// src/lib/flow-secrets.ts
export function isVariableReference(value: string): boolean;
export function plaintextSecretFields(auth: Auth): string[];
export function redactPlaintextSecrets(auth: Auth, replacement: string): { auth: Auth; fields: string[] };

// src/lib/url-variables.ts
export function secretKeysOf(
  vars: readonly Pick<Variable, 'key' | 'enabled' | 'secret'>[] | null | undefined,
): Set<string>;
// buildScopedContext params gain: envSecretKeys?: ReadonlySet<string>; globalSecretKeys?: ReadonlySet<string>;

// src/hooks/useCollectionVariableContext.ts
export interface CollectionVariableScope {
  variableContext: Map<string, VariableScopeEntry>;
  envVars: Record<string, string>;
  globalVars: Record<string, string>;
  collectionVars: CollectionVariable[];
  processEnvVars: Record<string, string>;
  activeEnvId: string | null;
  globalEnvName: string | null;
}
export function useCollectionVariableContext(collection: string): CollectionVariableScope;
```

Existing tests to know: `src/lib/__tests__/url-variables.test.ts` (only tests `buildResolver` and `sourceBadgeClass` today), `src/hooks/__tests__/useFolderVariableContext.test.tsx` (hook test with mocked queries), `src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx` (mocks `AuthEditor`, the environment queries and `getCollectionSettings`).

---

### Task 1: `flow-secrets.ts`

**Files:**
- Create: `src/lib/flow-secrets.ts`
- Create: `src/lib/__tests__/flow-secrets.test.ts`

**Interfaces:**
- Produces: `isVariableReference`, `plaintextSecretFields`, `redactPlaintextSecrets` as listed in the File Structure.

Semantics, fixed here so later plans can rely on them:

- `isVariableReference(value)` is true when the value is one or more `{{...}}` references and nothing else but whitespace, such as `{{token}}`, ` {{ a }} ` or `{{a}}{{b}}`. It is false for `Bearer {{token}}`, `abc`, an empty string and `{{a}`. (The design notes gave a single-reference regex. A concatenation of two variables holds no literal secret, so it is accepted too.)
- A field is plaintext when it is a string, not blank, and `isVariableReference` is false for it.
- `plaintextSecretFields` returns human labels, in a fixed order per auth type, never values.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/flow-secrets.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { Auth } from '@/lib/tauri-api';
import { isVariableReference, plaintextSecretFields, redactPlaintextSecrets } from '../flow-secrets';

const as = (auth: unknown) => auth as Auth;

describe('isVariableReference', () => {
  it.each(['{{token}}', ' {{ token }} ', '{{a}}{{b}}', '{{$guid}}', '{{vault.secret}}'])(
    'accepts %j',
    (value) => {
      expect(isVariableReference(value)).toBe(true);
    },
  );

  it.each(['', '   ', 'abc', 'Bearer {{token}}', '{{a}', 'x{{a}}', '{{a}} y', '{{}}'])(
    'rejects %j',
    (value) => {
      expect(isVariableReference(value)).toBe(false);
    },
  );
});

describe('plaintextSecretFields', () => {
  it('flags a literal bearer token only', () => {
    expect(plaintextSecretFields({ authType: 'bearer', token: 'abc123' })).toEqual(['Token']);
    expect(plaintextSecretFields({ authType: 'bearer', token: '{{token}}' })).toEqual([]);
    expect(plaintextSecretFields({ authType: 'bearer', token: '' })).toEqual([]);
    expect(plaintextSecretFields({ authType: 'bearer', token: '   ' })).toEqual([]);
  });

  it('does not flag the username, only the password', () => {
    const auth: Auth = { authType: 'basic', username: 'alice', password: 'hunter2' };
    expect(plaintextSecretFields(auth)).toEqual(['Password']);
    expect(plaintextSecretFields({ ...auth, password: '{{pw}}' })).toEqual([]);
  });

  it.each(['digest', 'wsse'] as const)('flags the %s password', (authType) => {
    expect(plaintextSecretFields({ authType, username: 'u', password: 'p4ss' })).toEqual([
      'Password',
    ]);
  });

  it('flags the NTLM password but not the domain', () => {
    expect(
      plaintextSecretFields({ authType: 'ntlm', username: 'u', password: 'p4ss', domain: 'corp' }),
    ).toEqual(['Password']);
  });

  it('flags the API key value but not its name', () => {
    expect(
      plaintextSecretFields({ authType: 'api-key', key: 'X-Key', value: 'k-123', placement: 'header' }),
    ).toEqual(['API key value']);
  });

  it('flags OAuth 2.0 client secret and resource owner password', () => {
    const auth = as({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      credentials: { clientId: 'cid', clientSecret: 'shh' },
      resourceOwner: { username: 'u', password: 'pw' },
    });
    expect(plaintextSecretFields(auth)).toEqual(['Client secret', 'Resource owner password']);
  });

  it('does not flag an OAuth 2.0 field that is a variable, or a token held in memory', () => {
    const auth = as({
      authType: 'o-auth2',
      flow: 'client_credentials',
      credentials: { clientId: 'cid', clientSecret: '{{secret}}' },
      accessToken: 'in-memory-token',
      refreshToken: 'in-memory-refresh',
    });
    expect(plaintextSecretFields(auth)).toEqual([]);
  });

  it('flags the AWS secret key and session token', () => {
    const auth = as({
      authType: 'aws-sig-v4',
      accessKey: 'AKIA',
      secretKey: 'sk',
      sessionToken: 'st',
      region: 'us-east-1',
      service: 's3',
    });
    expect(plaintextSecretFields(auth)).toEqual(['Secret access key', 'Session token']);
  });

  it('flags OAuth 1.0 secrets, and a private key only when it is inline text', () => {
    const base = {
      authType: 'o-auth1',
      consumerKey: 'ck',
      consumerSecret: 'cs',
      accessToken: 'at',
      accessTokenSecret: 'ats',
    };
    expect(plaintextSecretFields(as(base))).toEqual([
      'Consumer secret',
      'Access token',
      'Access token secret',
    ]);
    expect(
      plaintextSecretFields(as({ ...base, privateKey: { type: 'text', value: '-----BEGIN' } })),
    ).toContain('Private key');
    expect(
      plaintextSecretFields(as({ ...base, privateKey: { type: 'file', value: '/keys/a.pem' } })),
    ).not.toContain('Private key');
  });

  it('returns nothing for none and inherit', () => {
    expect(plaintextSecretFields({ authType: 'none' })).toEqual([]);
    expect(plaintextSecretFields({ authType: 'inherit' })).toEqual([]);
  });

  it('never returns a value', () => {
    const labels = plaintextSecretFields({ authType: 'bearer', token: 'super-secret-value' });
    expect(JSON.stringify(labels)).not.toContain('super-secret-value');
  });
});

describe('redactPlaintextSecrets', () => {
  it('replaces literal secrets and keeps variable references', () => {
    const auth = as({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      credentials: { clientId: 'cid', clientSecret: 'shh' },
      resourceOwner: { username: 'u', password: '{{pw}}' },
    });
    const out = redactPlaintextSecrets(auth, '<redacted>');
    expect(out.fields).toEqual(['Client secret']);
    expect(out.auth).toEqual({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      credentials: { clientId: 'cid', clientSecret: '<redacted>' },
      resourceOwner: { username: 'u', password: '{{pw}}' },
    });
  });

  it('does not change its input', () => {
    const auth: Auth = { authType: 'basic', username: 'u', password: 'p4ss' };
    redactPlaintextSecrets(auth, '<redacted>');
    expect(auth).toEqual({ authType: 'basic', username: 'u', password: 'p4ss' });
  });

  it('redacts an inline OAuth 1.0 private key but leaves a key file path', () => {
    const inline = as({ authType: 'o-auth1', privateKey: { type: 'text', value: 'PEM' } });
    expect(redactPlaintextSecrets(inline, 'X').auth).toEqual({
      authType: 'o-auth1',
      privateKey: { type: 'text', value: 'X' },
    });
    const file = as({ authType: 'o-auth1', privateKey: { type: 'file', value: '/k.pem' } });
    expect(redactPlaintextSecrets(file, 'X').auth).toEqual(file);
  });

  it('returns the same fields as plaintextSecretFields', () => {
    const auth: Auth = { authType: 'digest', username: 'u', password: 'p4ss' };
    expect(redactPlaintextSecrets(auth, 'X').fields).toEqual(plaintextSecretFields(auth));
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/lib/__tests__/flow-secrets.test.ts`
Expected: FAIL, cannot resolve `../flow-secrets`.

- [ ] **Step 3: Write the module**

Create `src/lib/flow-secrets.ts`:

```ts
import type { Auth } from '@/lib/tauri-api';

// One or more `{{variable}}` references and nothing else, such as `{{token}}`.
const ONLY_REFERENCES = /^\s*(?:\{\{[^{}]+\}\}\s*)+$/;

/** True when the value is only `{{variable}}` references, so it holds no literal secret. */
export function isVariableReference(value: string): boolean {
  return ONLY_REFERENCES.test(value);
}

type Path = readonly string[];

interface SecretField {
  label: string;
  path: Path;
  // Narrows a field to some shapes of the auth, such as an inline OAuth 1.0 key.
  when?: (auth: Auth) => boolean;
}

const PASSWORD: readonly SecretField[] = [{ label: 'Password', path: ['password'] }];

// The credential fields of each persisted auth type, in the order they are listed.
// The names are the flat persisted names, not the editor's nested `AuthState`.
const SECRET_FIELDS: Record<string, readonly SecretField[]> = {
  bearer: [{ label: 'Token', path: ['token'] }],
  basic: PASSWORD,
  digest: PASSWORD,
  wsse: PASSWORD,
  ntlm: PASSWORD,
  'api-key': [{ label: 'API key value', path: ['value'] }],
  'o-auth2': [
    { label: 'Client secret', path: ['credentials', 'clientSecret'] },
    { label: 'Resource owner password', path: ['resourceOwner', 'password'] },
  ],
  'aws-sig-v4': [
    { label: 'Secret access key', path: ['secretKey'] },
    { label: 'Session token', path: ['sessionToken'] },
  ],
  'o-auth1': [
    { label: 'Consumer secret', path: ['consumerSecret'] },
    { label: 'Access token', path: ['accessToken'] },
    { label: 'Access token secret', path: ['accessTokenSecret'] },
    {
      label: 'Private key',
      path: ['privateKey', 'value'],
      when: (auth) => readString(auth, ['privateKey', 'type']) === 'text',
    },
  ],
};

function readString(root: unknown, path: Path): string | undefined {
  let current: unknown = root;
  for (const part of path) {
    if (typeof current !== 'object' || current === null) return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return typeof current === 'string' ? current : undefined;
}

function isPlaintext(value: string | undefined): boolean {
  return value !== undefined && value.trim() !== '' && !isVariableReference(value);
}

// The fields of `auth` that hold a literal credential right now.
function plaintextFields(auth: Auth): SecretField[] {
  return (SECRET_FIELDS[auth.authType] ?? []).filter(
    (field) => (!field.when || field.when(auth)) && isPlaintext(readString(auth, field.path)),
  );
}

// A copy of `root` with the string at `path` replaced. Missing parents are not created.
function withString(root: Record<string, unknown>, path: Path, value: string): Record<string, unknown> {
  const [head, ...rest] = path;
  if (rest.length === 0) return { ...root, [head]: value };
  const child = root[head];
  if (typeof child !== 'object' || child === null) return root;
  return { ...root, [head]: withString(child as Record<string, unknown>, rest, value) };
}

/**
 * Labels of the credential fields in a persisted auth that hold literal text.
 * A field that is empty or only a `{{variable}}` reference is not listed.
 * Returns labels only, never values. An OAuth 2.0 token held in memory is not
 * part of the persisted auth and is never listed.
 */
export function plaintextSecretFields(auth: Auth): string[] {
  return plaintextFields(auth).map((field) => field.label);
}

/**
 * A copy of `auth` with every literal credential replaced by `replacement`, and
 * the labels of the fields that were replaced. `auth` itself is not changed.
 */
export function redactPlaintextSecrets(
  auth: Auth,
  replacement: string,
): { auth: Auth; fields: string[] } {
  const found = plaintextFields(auth);
  let next = auth as unknown as Record<string, unknown>;
  for (const field of found) next = withString(next, field.path, replacement);
  return { auth: next as unknown as Auth, fields: found.map((field) => field.label) };
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `yarn test src/lib/__tests__/flow-secrets.test.ts`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-secrets.ts src/lib/__tests__/flow-secrets.test.ts`
Suggested subject: `feat(flow): detect literal credentials in persisted auth`.

---

### Task 2: Carry the secret flag through `buildScopedContext` (F-57) - review in the main loop

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/url-variables.ts` (import line 1, `buildScopedContext` at lines 46-87)
- Modify: `src/components/request/RequestPanel.tsx:79,550`
- Modify: `src/components/collections/CollectionOverviewTab.tsx:37,147`
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx:19,134`
- Modify: `src/components/environments/EnvironmentDialog.tsx:34,412`
- Modify: `src/hooks/useGrpcVariableContext.ts:14,84`
- Modify: `src/hooks/useWebSocketVariableContext.ts:9,55`
- Modify: `src/hooks/useFolderVariableContext.ts:14,69`
- Modify: `src/components/flow/properties/AuthNodeEditor.tsx:33,115` (replaced wholesale in Task 3; fixed here so this commit leaves no open surface)
- Modify: `src/lib/__tests__/url-variables.test.ts`
- Create: `src/lib/__tests__/url-variables-callers.test.ts`
- Create: `src/components/editor/__tests__/VariablePopover.test.tsx`
- Modify: `src/hooks/__tests__/useFolderVariableContext.test.tsx`

**Interfaces:**
- Produces: `secretKeysOf(vars)` and the `envSecretKeys` and `globalSecretKeys` parameters, as in the File Structure.
- Consumes: nothing from Task 1.

- [ ] **Step 1: Write the failing tests for `buildScopedContext`**

In `src/lib/__tests__/url-variables.test.ts`, change the import on line 2 to:

```ts
import { buildResolver, buildScopedContext, secretKeysOf, sourceBadgeClass } from '../url-variables';
```

and append at the end of the file:

```ts
describe('secretKeysOf', () => {
  it('lists only enabled secret variables', () => {
    const keys = secretKeysOf([
      { key: 'apiKey', enabled: true, secret: true },
      { key: 'host', enabled: true, secret: false },
      { key: 'old', enabled: false, secret: true },
    ]);
    expect([...keys]).toEqual(['apiKey']);
  });

  it('accepts a missing list', () => {
    expect(secretKeysOf(undefined).size).toBe(0);
    expect(secretKeysOf(null).size).toBe(0);
  });
});

describe('buildScopedContext secret flag', () => {
  it('marks a secret environment variable as secret', () => {
    const ctx = buildScopedContext({
      envVars: { apiKey: 'sk-live-123', host: 'api.test' },
      envSecretKeys: new Set(['apiKey']),
    });
    expect(ctx.get('apiKey')?.secret).toBe(true);
    expect(ctx.get('apiKey')?.source).toBe('environment');
    expect(ctx.get('host')?.secret).toBe(false);
  });

  it('marks a secret global variable as secret', () => {
    const ctx = buildScopedContext({
      globalVars: { token: 'g-123' },
      globalSecretKeys: new Set(['token']),
    });
    expect(ctx.get('token')?.secret).toBe(true);
    expect(ctx.get('token')?.source).toBe('global');
  });

  it('keeps the real value on a secret entry so OAuth2 can still resolve it', () => {
    const ctx = buildScopedContext({
      envVars: { clientSecret: 'real-secret' },
      envSecretKeys: new Set(['clientSecret']),
    });
    expect(ctx.get('clientSecret')?.value).toBe('real-secret');
  });

  it('is unchanged when no secret keys are given', () => {
    const ctx = buildScopedContext({ envVars: { a: '1' }, globalVars: { b: '2' } });
    expect(ctx.get('a')?.secret).toBe(false);
    expect(ctx.get('b')?.secret).toBe(false);
  });

  it('does not let a higher non-secret layer inherit a lower secret flag', () => {
    const ctx = buildScopedContext({
      globalVars: { k: 'global-secret' },
      globalSecretKeys: new Set(['k']),
      envVars: { k: 'plain-env-value' },
    });
    // The environment layer wins and is not secret.
    expect(ctx.get('k')).toEqual(
      expect.objectContaining({ source: 'environment', value: 'plain-env-value', secret: false }),
    );
  });

  it('does not let a lower non-secret layer hide a higher secret flag', () => {
    const ctx = buildScopedContext({
      globalVars: { k: 'plain-global' },
      envVars: { k: 'env-secret' },
      envSecretKeys: new Set(['k']),
    });
    expect(ctx.get('k')).toEqual(
      expect.objectContaining({ source: 'environment', secret: true }),
    );
  });

  it('adds no entry for a variable that is not in the values', () => {
    const ctx = buildScopedContext({ envVars: {}, envSecretKeys: new Set(['ghost']) });
    expect(ctx.has('ghost')).toBe(false);
  });
});
```

- [ ] **Step 2: Write the failing popover test**

Create `src/components/editor/__tests__/VariablePopover.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { VariablePopover } from '../VariablePopover';

const SECRET = 'sk-live-do-not-show-123';

const entry = (over: Partial<VariableScopeEntry> = {}): VariableScopeEntry => ({
  value: SECRET,
  source: 'environment',
  label: 'dev',
  secret: false,
  ...over,
});

function renderPopover(e: VariableScopeEntry) {
  return render(
    <VariablePopover
      varName='apiKey'
      entry={e}
      tokenType='variable'
      onCommit={vi.fn(async () => undefined)}
      onClose={vi.fn()}
    />,
  );
}

describe('VariablePopover secret handling', () => {
  it('shows the value of a non-secret variable', () => {
    renderPopover(entry());
    expect(screen.getByRole('textbox')).toHaveValue(SECRET);
  });

  it('masks a secret variable and keeps the value out of the whole document', () => {
    renderPopover(entry({ secret: true }));
    const input = screen.getByRole('textbox');
    expect(input).toHaveValue('●●●●');
    expect(input).toHaveAttribute('readonly');
    expect(document.body.innerHTML).not.toContain(SECRET);
  });
});
```

- [ ] **Step 3: Write the failing caller audit test**

Create `src/lib/__tests__/url-variables-callers.test.ts`:

```ts
import { describe, expect, it } from 'vitest';

// Every file that builds a scoped context must pass the secret key sets, or the
// popover and the hover show a secret environment value in clear text (F-57).
const sources = import.meta.glob<string>('/src/**/*.{ts,tsx}', {
  query: '?raw',
  import: 'default',
  eager: true,
});

const isTest = (path: string) => path.includes('/__tests__/') || /\.test\.tsx?$/.test(path);

const callers = Object.entries(sources).filter(
  ([path, text]) =>
    !isTest(path) && !path.endsWith('/lib/url-variables.ts') && text.includes('buildScopedContext('),
);

describe('buildScopedContext callers', () => {
  it('finds the known callers', () => {
    expect(callers.length).toBeGreaterThanOrEqual(8);
  });

  it.each(callers)('%s passes envSecretKeys', (_path, text) => {
    expect(text).toContain('envSecretKeys');
  });

  it.each(callers.filter(([, text]) => text.includes('globalVars')))(
    '%s passes globalSecretKeys',
    (_path, text) => {
      expect(text).toContain('globalSecretKeys');
    },
  );
});
```

- [ ] **Step 4: Extend the folder hook test**

In `src/hooks/__tests__/useFolderVariableContext.test.tsx`:

1. Add after the `api` hoisted block (before the existing `vi.mock('@/lib/queries/environment-queries'...`):

```tsx
const envState = vi.hoisted(() => ({
  environments: [] as unknown[],
}));
```

2. Change the `useEnvironments` line in the existing environment-queries mock to:

```tsx
  useEnvironments: () => ({ data: envState.environments }),
```

3. Add the store import after the existing `import { useFolderVariableContext } ...` line:

```tsx
import { useEnvStore } from '@/stores/env-store';
```

4. Reset the state in the existing `beforeEach` (add these two lines at its end):

```tsx
    envState.environments = [];
    useEnvStore.setState({ activeEnvId: null, activeCollection: null });
```

5. Append inside the `describe`:

```tsx
  it('marks a secret environment variable as secret and keeps it unresolved by source', async () => {
    envState.environments = [
      {
        name: 'dev',
        variables: [
          { key: 'apiKey', value: 'sk-live-123', enabled: true, secret: true },
          { key: 'region', value: 'eu', enabled: true, secret: false },
        ],
      },
    ];
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'demo' });
    const { result } = renderHook(() => useFolderVariableContext('demo', '', []));
    await waitFor(() => expect(result.current.variableContext.get('host')).toBeDefined());
    expect(result.current.variableContext.get('apiKey')).toEqual(
      expect.objectContaining({ source: 'environment', secret: true }),
    );
    expect(result.current.variableContext.get('region')?.secret).toBe(false);
  });
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `yarn test src/lib/__tests__/url-variables.test.ts src/lib/__tests__/url-variables-callers.test.ts src/components/editor/__tests__/VariablePopover.test.tsx src/hooks/__tests__/useFolderVariableContext.test.tsx`
Expected: the `buildScopedContext secret flag` tests, the caller audit and the folder hook test FAIL (no such parameter or helper). The two `VariablePopover` tests PASS already: the popover masks correctly once the flag is set. They pin that behavior so a later change cannot unmask it.

- [ ] **Step 6: Implement the helper and the parameters**

In `src/lib/url-variables.ts`:

1. Change the first import line to:

```ts
import type { CollectionVariable, ExternalSecretBinding, Variable } from '@/lib/tauri-api';
```

2. Add this function after `buildResolver` and before the `buildScopedContext` comment block:

```ts
// The keys of the enabled variables flagged secret. Callers pass this next to
// the plain key-to-value map they already build, so the scoped context can
// mask those entries instead of showing the real value.
export function secretKeysOf(
  vars: readonly Pick<Variable, 'key' | 'enabled' | 'secret'>[] | null | undefined,
): Set<string> {
  return new Set((vars ?? []).filter((v) => v.enabled && v.secret).map((v) => v.key));
}
```

3. In the `buildScopedContext` parameter type, add after `envLabel?: string;`:

```ts
  /** Keys in `envVars` that are secret. Their entries keep the value but carry `secret: true`. */
  envSecretKeys?: ReadonlySet<string>;
```

and after `globalVars?: Record<string, string>;`:

```ts
  /** Keys in `globalVars` that are secret. */
  globalSecretKeys?: ReadonlySet<string>;
```

4. Replace the global loop and the environment loop:

```ts
  for (const [k, v] of Object.entries(params.globalVars ?? {}))
    add(k, v, 'global', 'Global', params.globalSecretKeys?.has(k) ?? false);
```

```ts
  for (const [k, v] of Object.entries(params.envVars ?? {}))
    add(k, v, 'environment', params.envLabel ?? 'Environment', params.envSecretKeys?.has(k) ?? false);
```

5. Extend the comment above the function with one line: `// Secret entries keep their value in memory (OAuth2 resolves from it) and are masked by the consumers that read entry.secret.`

- [ ] **Step 7: Update the eight callers**

In each file below, change the `url-variables` import to also bring in `secretKeysOf` (keep specifiers in the order Biome wants), and add the two properties to the `buildScopedContext({ ... })` call. Both sets are computed from the same objects the memo already reads, so no dependency array changes.

`src/components/request/RequestPanel.tsx` (call at line 550; `activeEnv` and `globalEnv` are in scope):

```ts
import { buildScopedContext, secretKeysOf } from '@/lib/url-variables';
```
```ts
      envVars,
      envSecretKeys: secretKeysOf(activeEnv?.variables),
      envLabel: activeEnvIdForScope ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars,
      globalSecretKeys: secretKeysOf(globalEnv?.variables),
```

`src/components/collections/CollectionOverviewTab.tsx` (call at line 147):

```ts
      envVars,
      envSecretKeys: secretKeysOf(activeEnv?.variables),
      envLabel: activeEnvId ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars,
      globalSecretKeys: secretKeysOf(globalEnv?.variables),
```

`src/components/workspace/WorkspaceEnvironmentsTab.tsx` (call at line 134; only the edited environment's variables apply, there is no global layer):

```ts
    return buildScopedContext({
      envVars,
      envSecretKeys: secretKeysOf(editingVars),
      envLabel: selectedName ?? undefined,
      processEnvVars,
    });
```

`src/components/environments/EnvironmentDialog.tsx` (call at line 412):

```ts
    return buildScopedContext({
      envVars,
      envSecretKeys: secretKeysOf(selectedEnv?.variables),
      envLabel: selectedEnv?.name,
      globalVars,
      globalSecretKeys: secretKeysOf(globalEnv?.variables),
      processEnvVars,
    });
```

`src/hooks/useGrpcVariableContext.ts` (call at line 84), `src/hooks/useWebSocketVariableContext.ts` (call at line 55) and `src/hooks/useFolderVariableContext.ts` (call at line 69) all have `activeEnv` and `globalEnv` in scope. Add to each call, next to `envVars` and `globalVars`:

```ts
      envSecretKeys: secretKeysOf(activeEnv?.variables),
      globalSecretKeys: secretKeysOf(globalEnv?.variables),
```

`src/components/flow/properties/AuthNodeEditor.tsx` (call at line 115; `activeEnv` and `globalEnv` are in scope; the dependency array already contains `activeEnv`, but add `globalEnv`):

```ts
      buildScopedContext({
        envVars,
        envSecretKeys: secretKeysOf(activeEnv?.variables),
        envLabel: activeEnvId ?? undefined,
        externalSecrets: activeEnv?.externalSecrets,
        globalVars,
        globalSecretKeys: secretKeysOf(globalEnv?.variables),
        processEnvVars,
        collectionVars,
      }),
    [activeEnvId, activeEnv, envVars, globalEnv, globalVars, processEnvVars, collectionVars],
```

- [ ] **Step 8: Run to verify the tests pass**

Run: `yarn test src/lib src/components/editor src/hooks src/components/flow/properties src/components/environments src/components/workspace src/components/collections`
Expected: PASS, including the caller audit (8 callers) and the existing tests of every touched component.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Manual check once, in the real app (`yarn tauri dev`): mark a variable secret in an environment, activate it, open a request, type `{{thatVariable}}` in the URL, click the token. The popover shows `●●●●` and is read-only. Record the result in the review note.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/url-variables.ts src/lib/__tests__/url-variables.test.ts src/lib/__tests__/url-variables-callers.test.ts src/components/editor/__tests__/VariablePopover.test.tsx src/hooks/__tests__/useFolderVariableContext.test.tsx src/components/request/RequestPanel.tsx src/components/collections/CollectionOverviewTab.tsx src/components/workspace/WorkspaceEnvironmentsTab.tsx src/components/environments/EnvironmentDialog.tsx src/hooks/useGrpcVariableContext.ts src/hooks/useWebSocketVariableContext.ts src/hooks/useFolderVariableContext.ts src/components/flow/properties/AuthNodeEditor.tsx`
Suggested subject: `fix(variables): mask secret environment values in the scoped context`.

---

### Task 3: `useCollectionVariableContext` and the plaintext warning

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/hooks/useCollectionVariableContext.ts`
- Create: `src/hooks/__tests__/useCollectionVariableContext.test.tsx`
- Modify: `src/components/flow/properties/AuthNodeEditor.tsx` (imports, lines 34-122 become one hook call, JSX before `<AuthEditor`)
- Modify: `src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx` (append one describe)

**Interfaces:**
- Produces: `useCollectionVariableContext(collection: string): CollectionVariableScope` as in the File Structure.
- Consumes: `secretKeysOf` and the `buildScopedContext` parameters from Task 2, and `plaintextSecretFields` from Task 1.

- [ ] **Step 1: Write the failing hook test**

Create `src/hooks/__tests__/useCollectionVariableContext.test.tsx`:

```tsx
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CollectionVariable } from '@/lib/tauri-api';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
}));
const queries = vi.hoisted(() => ({
  environments: [] as unknown[],
  globalEnv: null as unknown,
  globalName: null as string | null,
  processEnv: {} as Record<string, string>,
  seenCollection: null as string | null,
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: (collection: string | null) => {
    queries.seenCollection = collection;
    return { data: queries.environments };
  },
  useGlobalEnvironmentName: () => ({ data: queries.globalName }),
  useGlobalEnvironment: () => ({ data: queries.globalEnv }),
  useProcessEnvVars: () => ({ data: queries.processEnv }),
}));

import { useEnvStore } from '@/stores/env-store';
import { useCollectionVariableContext } from '../useCollectionVariableContext';

const cv = (key: string, value: string): CollectionVariable => ({
  key,
  value,
  initialValue: '',
  enabled: true,
  secret: false,
});

describe('useCollectionVariableContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getCollectionSettings.mockResolvedValue({ variables: [cv('tokenUrl', 'https://idp/token')] });
    queries.environments = [
      {
        name: 'dev',
        variables: [
          { key: 'clientId', value: 'dev-client', enabled: true, secret: false },
          { key: 'clientSecret', value: 'real-secret', enabled: true, secret: true },
          { key: 'off', value: 'x', enabled: false, secret: false },
        ],
        externalSecrets: [
          { alias: 'vault', connectionId: 'c', vaultName: 'v', secretNames: [{ name: 'k' }] },
        ],
      },
    ];
    queries.globalName = 'global';
    queries.globalEnv = {
      name: 'global',
      variables: [{ key: 'tenant', value: 'acme', enabled: true, secret: false }],
    };
    queries.processEnv = { HOME: '/home/u' };
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'other' });
  });

  it('layers process, global, collection and the active environment of the given collection', async () => {
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(result.current.variableContext.get('tokenUrl')).toBeDefined());
    const ctx = result.current.variableContext;
    expect(ctx.get('process.env.HOME')?.value).toBe('/home/u');
    expect(ctx.get('tenant')).toEqual(expect.objectContaining({ source: 'global', value: 'acme' }));
    expect(ctx.get('tokenUrl')).toEqual(expect.objectContaining({ source: 'collection' }));
    expect(ctx.get('clientId')).toEqual(
      expect.objectContaining({ source: 'environment', value: 'dev-client' }),
    );
    expect(ctx.has('off')).toBe(false);
    expect(ctx.get('vault.k')?.source).toBe('vault');
  });

  it('looks environments up in the collection argument, not the env store collection', () => {
    renderHook(() => useCollectionVariableContext('api'));
    expect(queries.seenCollection).toBe('api');
  });

  it('marks a secret environment variable as secret but keeps its value for resolution', async () => {
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(result.current.variableContext.get('tokenUrl')).toBeDefined());
    expect(result.current.variableContext.get('clientSecret')).toEqual(
      expect.objectContaining({ secret: true, value: 'real-secret' }),
    );
    expect(result.current.envVars.clientSecret).toBe('real-secret');
  });

  it('returns the plain maps and names the editors need for token keys', async () => {
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(result.current.collectionVars).toHaveLength(1));
    expect(result.current.envVars).toEqual({
      clientId: 'dev-client',
      clientSecret: 'real-secret',
    });
    expect(result.current.globalVars).toEqual({ tenant: 'acme' });
    expect(result.current.processEnvVars).toEqual({ HOME: '/home/u' });
    expect(result.current.activeEnvId).toBe('dev');
    expect(result.current.globalEnvName).toBe('global');
  });

  it('falls back to no collection variables when settings fail to load', async () => {
    api.getCollectionSettings.mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(api.getCollectionSettings).toHaveBeenCalled());
    expect(result.current.collectionVars).toEqual([]);
    expect(result.current.variableContext.get('clientId')).toBeDefined();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/hooks/__tests__/useCollectionVariableContext.test.tsx`
Expected: FAIL, cannot resolve `../useCollectionVariableContext`.

- [ ] **Step 3: Write the hook**

Create `src/hooks/useCollectionVariableContext.ts`:

```ts
import { useEffect, useMemo, useState } from 'react';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import { type CollectionVariable, type Environment, getCollectionSettings } from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf, type VariableScopeEntry } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';

// Stable defaults while a query has no data, so the memoized values below are
// not rebuilt on every render.
const NO_ENVIRONMENTS: Environment[] = [];
const NO_PROCESS_ENV: Record<string, string> = {};

/** The variable scope of one collection, as the flow editors need it. */
export interface CollectionVariableScope {
  /** Scope-aware map for highlighting, autocomplete, popovers and hover. */
  variableContext: Map<string, VariableScopeEntry>;
  /** Enabled variables of the active environment, key to real value. For resolution only. */
  envVars: Record<string, string>;
  /** Enabled variables of the active global environment, key to real value. */
  globalVars: Record<string, string>;
  collectionVars: CollectionVariable[];
  processEnvVars: Record<string, string>;
  /** The active environment's name, or null. */
  activeEnvId: string | null;
  /** The active global environment's name, or null. */
  globalEnvName: string | null;
}

/**
 * The variables a flow sees for `collection`: process, global, the collection's own,
 * and the active environment looked up in that collection (not in the env store's
 * active collection, which can be a different one). This is the same layering the
 * pre-run step uses, so the editor and the run agree.
 */
export function useCollectionVariableContext(collection: string): CollectionVariableScope {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: environments = NO_ENVIRONMENTS } = useEnvironments(collection);
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = NO_PROCESS_ENV } = useProcessEnvVars();
  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);

  useEffect(() => {
    let cancelled = false;
    getCollectionSettings(collection)
      .then((s) => {
        if (!cancelled) setCollectionVars(s.variables);
      })
      .catch(() => {
        if (!cancelled) setCollectionVars([]);
      });
    return () => {
      cancelled = true;
    };
  }, [collection]);

  const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
  const envVars = useMemo(() => {
    const vars: Record<string, string> = {};
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) vars[v.key] = v.value;
    return vars;
  }, [activeEnv]);
  const globalVars = useMemo<Record<string, string>>(
    () =>
      globalEnv
        ? Object.fromEntries(
            globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
          )
        : {},
    [globalEnv],
  );

  const variableContext = useMemo(
    () =>
      buildScopedContext({
        envVars,
        envSecretKeys: secretKeysOf(activeEnv?.variables),
        envLabel: activeEnvId ?? undefined,
        externalSecrets: activeEnv?.externalSecrets,
        globalVars,
        globalSecretKeys: secretKeysOf(globalEnv?.variables),
        processEnvVars,
        collectionVars,
      }),
    [activeEnvId, activeEnv, envVars, globalEnv, globalVars, processEnvVars, collectionVars],
  );

  return {
    variableContext,
    envVars,
    globalVars,
    collectionVars,
    processEnvVars,
    activeEnvId,
    globalEnvName,
  };
}
```

- [ ] **Step 4: Run to verify the hook tests pass**

Run: `yarn test src/hooks/__tests__/useCollectionVariableContext.test.tsx`
Expected: PASS.

- [ ] **Step 5: Write the failing warning tests**

Append to `src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`, as a new `describe` at the end of the file (after the closing of the top-level `describe('AuthNodeEditor', ...)`), reusing the file's `AuthKind`, `Auth` and `kind` definitions:

```tsx
describe('AuthNodeEditor plaintext credential warning', () => {
  const SECRET = 'hunter2-literal-value';
  const renderKind = (auth: Auth) =>
    render(
      <AuthNodeEditor
        kind={{ ...kind, auth }}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );

  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    useEnvStore.setState({ activeEnvId: null, activeCollection: null });
  });

  it('warns about a literal password and names the field, never the value', () => {
    renderKind({ authType: 'basic', username: 'u', password: SECRET });
    const note = screen.getByRole('note');
    expect(note).toHaveTextContent(
      'This credential is saved as plain text in the flow file. Use a {{variable}} or a RocketVault reference instead.',
    );
    expect(note).toHaveTextContent('Password');
    expect(document.body.innerHTML).not.toContain(SECRET);
  });

  it('lists every literal field of an OAuth 2.0 auth', () => {
    renderKind({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      accessTokenUrl: '{{tokenUrl}}',
      credentials: { clientId: '{{clientId}}', clientSecret: SECRET },
      resourceOwner: { username: 'u', password: 'also-literal' },
    } as unknown as Auth);
    const note = screen.getByRole('note');
    expect(note).toHaveTextContent('Client secret');
    expect(note).toHaveTextContent('Resource owner password');
    expect(document.body.innerHTML).not.toContain(SECRET);
    expect(document.body.innerHTML).not.toContain('also-literal');
  });

  it('does not warn when the credential is a variable reference', () => {
    renderKind({ authType: 'basic', username: 'u', password: '{{password}}' });
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });

  it('does not warn for an empty credential', () => {
    renderKind({ authType: 'bearer', token: '' });
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 6: Run to verify the warning tests fail**

Run: `yarn test src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`
Expected: the first two warning tests FAIL (no note). All earlier tests PASS (the editor still uses its inline code).

- [ ] **Step 7: Refactor `AuthNodeEditor` onto the hook and add the warning**

In `src/components/flow/properties/AuthNodeEditor.tsx`:

1. Replace the import block so it matches exactly what the component still uses. The React import becomes `import { useCallback, useMemo } from 'react';`. Remove the imports of `useEnvironments`, `useGlobalEnvironment`, `useGlobalEnvironmentName`, `useProcessEnvVars`, `buildScopedContext`, `useEnvStore`, and the `CollectionVariable`, `Environment` and `getCollectionSettings` names (keep `type FlowNodeKind`). Add:

```tsx
import { useCollectionVariableContext } from '@/hooks/useCollectionVariableContext';
import { plaintextSecretFields } from '@/lib/flow-secrets';
```

2. Delete the `NO_ENVIRONMENTS` and `NO_PROCESS_ENV` constants and their comment.

3. Replace everything in the component body from `const activeEnvId = useEnvStore(...)` down to and including the `variableContext` `useMemo` (the old lines 60-122) with:

```tsx
  // The flow's own collection, which the pre-run step also uses
  // (buildOAuth2VarContext), so both resolve the same environment. The hook is
  // also the single source of the editor's highlighting and OAuth2 resolution.
  const {
    variableContext,
    envVars,
    globalVars,
    collectionVars,
    processEnvVars,
    activeEnvId,
    globalEnvName,
  } = useCollectionVariableContext(collection);
  const applyBlocked = otherNodeApplies && !kind.applyToInherit;
  const key = flowAuthKey(collection, flowName, nodeId, activeEnvId, globalEnvName);
  const stored = useFlowAuthStore((s) => s.auths[key]);
  const setAuth = useFlowAuthStore((s) => s.setAuth);
  const environmentName = activeEnvId ?? undefined;
  // Literal credentials in the persisted auth, by label. Never the values.
  const plaintextFields = plaintextSecretFields(kind.auth);
```

4. Leave the `rv`, `state` and `handleAuthChange` blocks as they are.

5. In the JSX, between the closing `</div>` of the "Auth type" block and `<AuthEditor`, add:

```tsx
      {plaintextFields.length > 0 && (
        <p role='note' className='text-xs text-amber-600 dark:text-amber-500'>
          This credential is saved as plain text in the flow file. Use a {'{{variable}}'} or a
          RocketVault reference instead. Plain text: {plaintextFields.join(', ')}.
        </p>
      )}
```

- [ ] **Step 8: Run to verify all editor tests pass**

Run: `yarn test src/components/flow src/hooks`
Expected: PASS. In particular the existing `AuthNodeEditor` token tests (fingerprint, global environment key, vault reference) prove the refactor kept the same keys and resolution.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors (an unused import left behind fails `yarn check`).

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/hooks/useCollectionVariableContext.ts src/hooks/__tests__/useCollectionVariableContext.test.tsx src/components/flow/properties/AuthNodeEditor.tsx src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`
Suggested subject: `feat(flow): share the collection variable hook and warn on plaintext credentials`.

---

## Self-Review

- **Spec coverage:** F-57 (Task 2, all eight callers, caller audit, popover DOM check). F-47 warning (Task 1 detection, Task 3 note with the exact wording from the design notes plus the field list). F-43 prerequisite (Task 3 hook). `redactPlaintextSecrets` is extra to the roadmap and exists for plan P15.
- **Placeholders:** none. Every step shows code or an exact edit.
- **Type consistency:** `plaintextSecretFields(auth: Auth)` takes the persisted `Auth` in Tasks 1 and 3 (the design notes said `AuthState`; corrected above). `secretKeysOf` and the two `buildScopedContext` parameters have the same names in Task 2, in the hook and in the audit test. `CollectionVariableScope` fields match the destructuring in `AuthNodeEditor` and in the hook test.
- **Review Focus coverage:** item 1 is the audit test, item 2 the `keeps the real value` test, item 3 the two layering tests plus `adds no entry`, item 4 the `VariablePopover` document check, item 5 the Task 1 field tests and the Task 3 DOM checks.
- **Known gap:** the click popover still edits the environment of `useEnvStore.activeCollection`, not the flow's collection. That is fixed for flow editors in plan P13 (`readOnlyVariables`). The `AuthEditor` fields inside `AuthNodeEditor` still use the old popover edit path; out of scope here.
