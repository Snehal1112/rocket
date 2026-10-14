# Console secret masking: helper, secret set and send wiring Implementation Plan

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. Steps use checkbox (`- [ ]`) syntax for tracking. Do not run any commit step: the driver commits.

**Goal:** Build the pure masking helper, collect the secret values of a send, and wire both into `sendRequest` so every Console HTTP row carries a masked copy fixed at send time.

**Architecture:** A new pure module `src/lib/console-redaction.ts` masks an HTTP exchange by secret value (6-character minimum, longest first), by sensitive header name (value only), by URL credentials and by an API-key query parameter. `resolveRequestFieldsForPath` returns the secret values and an uncertainty flag. `sendRequest` calls the helper on the success and error paths and stores the result in two new optional fields of `HttpConsoleEntry`. The Console panel (plan 2) only reads those fields.

**Tech Stack:** TypeScript, Zustand, Vitest, Biome. Frontend only, no Rust change.

**Spec:** `docs/superpowers/specs/2026-10-15-console-mask-secrets-design.md`

## Global Constraints

- Marker: `••••••`. Minimum secret length: 6 characters (`MIN_REDACTION_LEN`), shorter secrets stay visible.
- Sensitive headers, matched case-insensitively, name stays visible and only the value is masked: `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, `X-Api-Key`.
- Also masked: credentials in the URL (`user:pass@host`) and an API key sent as a query parameter.
- A value from a secret manager is never resolved, shown or stored raw by the frontend: the helper has no input for it, and an unresolved `{{provider.name}}` template passes through unchanged.
- Fail closed: when the secret set cannot be built, `uncertain` is true and the structural masks still apply.
- Only the single-request send path changes. Flow rows (`FlowPane.tsx`), the Runner, history and all Rust redaction are untouched.
- No raw secret is written to disk. No unwrap-style shortcuts, no new dependency. Use `yarn` only.
- Run checks from the repository root. Frontend checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`.

## Review Focus

- A secret that contains a shorter secret must mask fully, not leave a tail. Pinned by `masks the longest secret first` in Task 1.
- A secret of 5 characters must stay visible and a 6-character one must be masked. Pinned by `keeps secrets shorter than 6 characters` and `masks a 6 character secret` in Task 1.
- Header names must match case-insensitively and an empty sensitive header must stay empty. Pinned by `masks sensitive header values whatever the case` and `keeps an empty sensitive header empty` in Task 1.
- A `{{vault.token}}` template in the URL must come out unchanged and must not count as a change. Pinned by `leaves an unresolved template untouched` in Task 1.
- A missing environment in the query cache must set the uncertainty flag and an empty cache must not crash. Pinned by `flags the secret set as uncertain when the environment is not cached` in Task 2.

---

### Task 1: The masking helper

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/lib/console-redaction.ts`
- Test: `src/lib/__tests__/console-redaction.test.ts`

**Interfaces:**
- Consumes: `Auth` from `@/lib/tauri-api` (type only).
- Produces, all exported from `src/lib/console-redaction.ts`:
  - `REDACTION_MARKER: string`, `MIN_REDACTION_LEN: number`
  - `HeaderPair = { key: string; value: string }`
  - `HttpExchangeText = { url: string; requestHeaders: HeaderPair[]; requestBody: string; responseHeaders: HeaderPair[]; responseBody: string }`
  - `SecretVariable = { value: string; initialValue?: string; enabled: boolean; secret: boolean }`
  - `collectSecretValues(lists: SecretVariable[][], extra?: string[]): string[]`
  - `authSecretValues(auth: Auth): string[]`
  - `MaskOptions = { secretValues: string[]; apiKeyQueryParam?: string; uncertain?: boolean }`
  - `buildMaskOptions(auth: Auth, secretValues: string[], uncertain: boolean): MaskOptions`
  - `MaskedExchange = { masked: HttpExchangeText; changed: boolean; uncertain: boolean }`
  - `maskHttpExchange(input: HttpExchangeText, options: MaskOptions): MaskedExchange`
  - `selectHttpView(entry, revealed: boolean): HttpExchangeText` and `hasHiddenValues(entry): boolean`, where `entry` is `HttpExchangeText & { masked?: HttpExchangeText; uncertain?: boolean }`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/console-redaction.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  authSecretValues,
  buildMaskOptions,
  collectSecretValues,
  hasHiddenValues,
  type HttpExchangeText,
  maskHttpExchange,
  REDACTION_MARKER as M,
  selectHttpView,
} from '../console-redaction';

function exchange(overrides: Partial<HttpExchangeText> = {}): HttpExchangeText {
  return {
    url: 'https://x.test/a',
    requestHeaders: [],
    requestBody: '',
    responseHeaders: [],
    responseBody: '',
    ...overrides,
  };
}

describe('maskHttpExchange', () => {
  it('masks a secret value in the url, headers and bodies of the request and the response', () => {
    const out = maskHttpExchange(
      exchange({
        url: 'https://x.test/a?k=s3cr3t-value',
        requestHeaders: [{ key: 'X-Custom', value: 'a s3cr3t-value b' }],
        requestBody: '{"t":"s3cr3t-value"}',
        responseHeaders: [{ key: 'X-Echo', value: 's3cr3t-value' }],
        responseBody: 's3cr3t-value',
      }),
      { secretValues: ['s3cr3t-value'] },
    );
    expect(out.masked.url).toBe(`https://x.test/a?k=${M}`);
    expect(out.masked.requestHeaders).toEqual([{ key: 'X-Custom', value: `a ${M} b` }]);
    expect(out.masked.requestBody).toBe(`{"t":"${M}"}`);
    expect(out.masked.responseHeaders).toEqual([{ key: 'X-Echo', value: M }]);
    expect(out.masked.responseBody).toBe(M);
    expect(out.changed).toBe(true);
    expect(out.uncertain).toBe(false);
  });

  it('keeps secrets shorter than 6 characters', () => {
    const out = maskHttpExchange(exchange({ requestBody: 'abc12' }), { secretValues: ['abc12'] });
    expect(out.masked.requestBody).toBe('abc12');
    expect(out.changed).toBe(false);
  });

  it('masks a 6 character secret', () => {
    const out = maskHttpExchange(exchange({ requestBody: 'abc123' }), { secretValues: ['abc123'] });
    expect(out.masked.requestBody).toBe(M);
  });

  it('masks the longest secret first', () => {
    const out = maskHttpExchange(exchange({ requestBody: 'abcdefghij' }), {
      secretValues: ['abcdef', 'abcdefghij'],
    });
    expect(out.masked.requestBody).toBe(M);
  });

  it('masks sensitive header values whatever the case and keeps the names', () => {
    const out = maskHttpExchange(
      exchange({
        requestHeaders: [
          { key: 'AUTHORIZATION', value: 'Bearer xyz' },
          { key: 'cookie', value: 'a=b' },
          { key: 'Proxy-Authorization', value: 'Basic q' },
          { key: 'x-api-key', value: 'k' },
          { key: 'Accept', value: 'json' },
        ],
        responseHeaders: [{ key: 'Set-Cookie', value: 's=1' }],
      }),
      { secretValues: [] },
    );
    expect(out.masked.requestHeaders).toEqual([
      { key: 'AUTHORIZATION', value: M },
      { key: 'cookie', value: M },
      { key: 'Proxy-Authorization', value: M },
      { key: 'x-api-key', value: M },
      { key: 'Accept', value: 'json' },
    ]);
    expect(out.masked.responseHeaders).toEqual([{ key: 'Set-Cookie', value: M }]);
  });

  it('keeps an empty sensitive header empty', () => {
    const out = maskHttpExchange(
      exchange({ requestHeaders: [{ key: 'Authorization', value: '' }] }),
      { secretValues: [] },
    );
    expect(out.masked.requestHeaders).toEqual([{ key: 'Authorization', value: '' }]);
  });

  it('masks credentials in the url', () => {
    const out = maskHttpExchange(exchange({ url: 'https://user:pass@host.test/p' }), {
      secretValues: [],
    });
    expect(out.masked.url).toBe(`https://${M}@host.test/p`);
  });

  it('masks the value of the api key query parameter only', () => {
    const out = maskHttpExchange(
      exchange({ url: 'https://h.test/p?api_key=abc&x=1&api_key2=zz#frag' }),
      { secretValues: [], apiKeyQueryParam: 'api_key' },
    );
    expect(out.masked.url).toBe(`https://h.test/p?api_key=${M}&x=1&api_key2=zz#frag`);
  });

  it('leaves an unresolved template untouched', () => {
    const out = maskHttpExchange(exchange({ url: 'https://h.test/{{vault.token}}' }), {
      secretValues: [],
    });
    expect(out.masked.url).toBe('https://h.test/{{vault.token}}');
    expect(out.changed).toBe(false);
  });

  it('passes the uncertain flag through and still masks the sensitive headers', () => {
    const out = maskHttpExchange(
      exchange({ requestHeaders: [{ key: 'Authorization', value: 'Bearer abc' }] }),
      { secretValues: [], uncertain: true },
    );
    expect(out.uncertain).toBe(true);
    expect(out.masked.requestHeaders).toEqual([{ key: 'Authorization', value: M }]);
  });
});

describe('collectSecretValues', () => {
  it('keeps enabled secret values of 6 or more characters, longest first', () => {
    const out = collectSecretValues([
      [
        { value: 'short', enabled: true, secret: true },
        { value: 'abcdef', enabled: true, secret: true },
        { value: 'plain-value', enabled: true, secret: false },
        { value: 'disabled-secret', enabled: false, secret: true },
      ],
      [{ value: '', initialValue: 'fallback-secret', enabled: true, secret: true }],
    ]);
    expect(out).toEqual(['fallback-secret', 'abcdef']);
  });

  it('adds the extra values and drops duplicates', () => {
    const out = collectSecretValues([[{ value: 'abcdef', enabled: true, secret: true }]], [
      'abcdef',
      'tok-123456',
    ]);
    expect(out).toEqual(['tok-123456', 'abcdef']);
  });
});

describe('authSecretValues and buildMaskOptions', () => {
  it('returns the credential of each auth kind', () => {
    expect(authSecretValues({ authType: 'bearer', token: 'tok' })).toEqual(['tok']);
    expect(authSecretValues({ authType: 'basic', username: 'u', password: 'pw' })).toEqual(['pw']);
    expect(
      authSecretValues({ authType: 'api-key', key: 'k', value: 'v', placement: 'header' }),
    ).toEqual(['v']);
    expect(authSecretValues({ authType: 'none' })).toEqual([]);
  });

  it('sets the api key query parameter only for query placement', () => {
    const query = buildMaskOptions(
      { authType: 'api-key', key: 'api_key', value: 'abcdefgh', placement: 'query' },
      [],
      false,
    );
    expect(query.apiKeyQueryParam).toBe('api_key');
    expect(query.secretValues).toContain('abcdefgh');
    const header = buildMaskOptions(
      { authType: 'api-key', key: 'X-K', value: 'abcdefgh', placement: 'header' },
      [],
      true,
    );
    expect(header.apiKeyQueryParam).toBeUndefined();
    expect(header.uncertain).toBe(true);
  });
});

describe('selectHttpView and hasHiddenValues', () => {
  const raw = exchange({ requestBody: 'abcdefgh' });
  const masked = exchange({ requestBody: M });

  it('shows the masked copy unless revealed', () => {
    expect(selectHttpView({ ...raw, masked }, false).requestBody).toBe(M);
    expect(selectHttpView({ ...raw, masked }, true).requestBody).toBe('abcdefgh');
  });

  it('shows the raw fields when there is no masked copy', () => {
    expect(selectHttpView(raw, false).requestBody).toBe('abcdefgh');
  });

  it('never reveals an uncertain row', () => {
    expect(selectHttpView({ ...raw, masked, uncertain: true }, true).requestBody).toBe(M);
  });

  it('reports hidden values only when the masked copy differs', () => {
    expect(hasHiddenValues({ ...raw, masked })).toBe(true);
    expect(hasHiddenValues({ ...raw, masked: raw })).toBe(false);
    expect(hasHiddenValues(raw)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the tests and see them fail**

Run: `yarn test console-redaction`
Expected: FAIL, the module `../console-redaction` cannot be found.

- [ ] **Step 3: Write the helper**

Create `src/lib/console-redaction.ts`:

```ts
import type { Auth } from '@/lib/tauri-api';

export const REDACTION_MARKER = '••••••';
// Same minimum as history and the script engine: a shorter value would blank unrelated text.
export const MIN_REDACTION_LEN = 6;

// Header values hidden by name. The name stays visible.
const SENSITIVE_HEADER_NAMES = new Set([
  'authorization',
  'proxy-authorization',
  'cookie',
  'set-cookie',
  'x-api-key',
]);

export interface HeaderPair {
  key: string;
  value: string;
}

export interface HttpExchangeText {
  url: string;
  requestHeaders: HeaderPair[];
  requestBody: string;
  responseHeaders: HeaderPair[];
  responseBody: string;
}

export interface SecretVariable {
  value: string;
  initialValue?: string;
  enabled: boolean;
  secret: boolean;
}

export interface MaskOptions {
  secretValues: string[];
  /** Name of the API key sent as a query parameter. */
  apiKeyQueryParam?: string;
  /** True when the secret set could not be built. */
  uncertain?: boolean;
}

export interface MaskedExchange {
  masked: HttpExchangeText;
  changed: boolean;
  uncertain: boolean;
}

// Keeps the enabled secret values that are long enough, longest first so that a secret
// containing a shorter one is replaced whole.
export function collectSecretValues(lists: SecretVariable[][], extra: string[] = []): string[] {
  const found = new Set<string>();
  for (const list of lists) {
    for (const variable of list) {
      if (!variable.enabled || !variable.secret) continue;
      found.add(variable.value || variable.initialValue || '');
    }
  }
  for (const value of extra) found.add(value);
  return [...found].filter((v) => v.length >= MIN_REDACTION_LEN).sort((a, b) => b.length - a.length);
}

// The credentials an auth block adds to a send.
export function authSecretValues(auth: Auth): string[] {
  switch (auth.authType) {
    case 'bearer':
      return [auth.token];
    case 'basic':
    case 'digest':
    case 'wsse':
    case 'ntlm':
      return [auth.password];
    case 'api-key':
      return [auth.value];
    default:
      return [];
  }
}

export function buildMaskOptions(
  auth: Auth,
  secretValues: string[],
  uncertain: boolean,
): MaskOptions {
  return {
    secretValues: [...secretValues, ...authSecretValues(auth)],
    apiKeyQueryParam:
      auth.authType === 'api-key' && auth.placement === 'query' ? auth.key : undefined,
    uncertain,
  };
}

function maskText(text: string, secrets: string[]): string {
  let out = text;
  for (const secret of secrets) out = out.split(secret).join(REDACTION_MARKER);
  return out;
}

function maskHeaders(headers: HeaderPair[], secrets: string[]): HeaderPair[] {
  return headers.map((h) => {
    if (SENSITIVE_HEADER_NAMES.has(h.key.toLowerCase())) {
      return { key: h.key, value: h.value ? REDACTION_MARKER : h.value };
    }
    return { key: h.key, value: maskText(h.value, secrets) };
  });
}

const URL_USERINFO = /^([a-z][a-z0-9+.-]*:\/\/)[^/?#@]+@/i;

function maskApiKeyParam(url: string, name: string | undefined): string {
  if (!name) return url;
  const queryStart = url.indexOf('?');
  if (queryStart < 0) return url;
  const hash = url.indexOf('#', queryStart);
  const end = hash < 0 ? url.length : hash;
  const query = url
    .slice(queryStart + 1, end)
    .split('&')
    .map((pair) => {
      const eq = pair.indexOf('=');
      if (eq < 0) return pair;
      const rawKey = pair.slice(0, eq);
      let key = rawKey;
      try {
        key = decodeURIComponent(rawKey.replace(/\+/g, ' '));
      } catch {
        // Keep the raw key when it is not valid percent-encoding.
      }
      return key === name ? `${rawKey}=${REDACTION_MARKER}` : pair;
    })
    .join('&');
  return `${url.slice(0, queryStart + 1)}${query}${url.slice(end)}`;
}

function maskUrl(url: string, secrets: string[], apiKeyParam: string | undefined): string {
  const byValue = maskText(url, secrets);
  const noCredentials = byValue.replace(URL_USERINFO, `$1${REDACTION_MARKER}@`);
  return maskApiKeyParam(noCredentials, apiKeyParam);
}

export function maskHttpExchange(input: HttpExchangeText, options: MaskOptions): MaskedExchange {
  const secrets = collectSecretValues([], options.secretValues);
  const masked: HttpExchangeText = {
    url: maskUrl(input.url, secrets, options.apiKeyQueryParam),
    requestHeaders: maskHeaders(input.requestHeaders, secrets),
    requestBody: maskText(input.requestBody, secrets),
    responseHeaders: maskHeaders(input.responseHeaders, secrets),
    responseBody: maskText(input.responseBody, secrets),
  };
  return {
    masked,
    changed: JSON.stringify(masked) !== JSON.stringify(input),
    uncertain: options.uncertain ?? false,
  };
}

type ViewableEntry = HttpExchangeText & { masked?: HttpExchangeText; uncertain?: boolean };

function rawFields(entry: HttpExchangeText): HttpExchangeText {
  return {
    url: entry.url,
    requestHeaders: entry.requestHeaders,
    requestBody: entry.requestBody,
    responseHeaders: entry.responseHeaders,
    responseBody: entry.responseBody,
  };
}

// The text a row shows. An uncertain row is never revealed.
export function selectHttpView(entry: ViewableEntry, revealed: boolean): HttpExchangeText {
  if (!entry.masked) return rawFields(entry);
  if (revealed && !entry.uncertain) return rawFields(entry);
  return entry.masked;
}

export function hasHiddenValues(entry: ViewableEntry): boolean {
  return !!entry.masked && JSON.stringify(entry.masked) !== JSON.stringify(rawFields(entry));
}
```

- [ ] **Step 4: Run the tests and see them pass**

Run: `yarn test console-redaction`
Expected: PASS, all tests green.

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: no errors. If Biome reports formatting only, run `yarn format` once and re-run `yarn check`.

---

### Task 2: Entry fields and the secret set

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/stores/console-store.ts` (the `HttpConsoleEntry` interface, lines 7 to 22)
- Modify: `src/lib/execute-request.ts` (imports, `ResolvedRequestFields` at line 223, `resolveRequestFieldsForPath` at lines 258 to 417)
- Test: `src/stores/__tests__/console-store.masked.test.ts` (create)
- Test: `src/lib/__tests__/execute-request.secrets.test.ts` (create)

**Interfaces:**
- Consumes: from Task 1, `HttpExchangeText`, `collectSecretValues`.
- Produces:
  - `HttpConsoleEntry.masked?: HttpExchangeText` and `HttpConsoleEntry.uncertain?: boolean`; `addHttpEntry` keeps both.
  - `ResolvedRequestFields.secretValues?: string[]` and `ResolvedRequestFields.secretsUncertain?: boolean`, filled by `resolveRequestFieldsForPath`.

- [ ] **Step 1: Write the failing store test**

Create `src/stores/__tests__/console-store.masked.test.ts` (create the folder only if it does not exist):

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '../console-store';

const base = {
  method: 'GET',
  url: 'https://x.test/a',
  status: 200,
  statusText: 'OK',
  durationMs: 1,
  sizeBytes: 1,
  requestHeaders: [],
  requestBody: 'secret-body',
  responseHeaders: [],
  responseBody: '',
};

describe('console store masked copy', () => {
  beforeEach(() => useConsoleStore.getState().clearEntries());

  it('keeps the masked copy and the uncertain flag on an http entry', () => {
    const masked = { ...base, requestBody: '••••••' };
    useConsoleStore.getState().addHttpEntry({ ...base, masked, uncertain: true });
    const entry = useConsoleStore.getState().entries[0];
    expect(entry.kind).toBe('http');
    if (entry.kind === 'http') {
      expect(entry.masked?.requestBody).toBe('••••••');
      expect(entry.uncertain).toBe(true);
      expect(entry.requestBody).toBe('secret-body');
    }
  });

  it('leaves both fields absent for an entry that was not masked', () => {
    useConsoleStore.getState().addHttpEntry(base);
    const entry = useConsoleStore.getState().entries[0];
    if (entry.kind === 'http') {
      expect(entry.masked).toBeUndefined();
      expect(entry.uncertain).toBeUndefined();
    }
  });
});
```

- [ ] **Step 2: Write the failing secret-set test**

Create `src/lib/__tests__/execute-request.secrets.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';

const state = vi.hoisted(() => ({ cache: new Map<string, unknown>() }));

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({ variables: [], headers: [] })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: 'dev', activeCollection: 'demo' }) },
}));

vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: { getState: () => ({ getCollectionAuth: () => undefined }) },
}));

vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({
    getQueryData: (key: readonly unknown[]) => state.cache.get(JSON.stringify(key)),
  }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { getCollectionSettings } from '@/lib/tauri-api';

function request(): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: 'https://x.test/a',
    pathParams: [],
    queryParams: [],
    headers: [],
    body: { mode: 'none', content: '', formData: [] },
    auth: { authType: 'none' },
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
  };
}

const devEnv = {
  name: 'dev',
  variables: [
    { key: 'API_TOKEN', value: 'tok-123456', enabled: true, secret: true },
    { key: 'SHORT', value: 'abc', enabled: true, secret: true },
    { key: 'HOST', value: 'h.test', enabled: true, secret: false },
  ],
};

describe('secret values of a send', () => {
  beforeEach(() => {
    state.cache.clear();
    vi.clearAllMocks();
  });

  it('collects the secret values of the active environment', async () => {
    state.cache.set(JSON.stringify(['environments', 'demo']), [devEnv]);
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['tok-123456']);
    expect(resolved.secretsUncertain).toBe(false);
  });

  it('collects secret collection variables', async () => {
    state.cache.set(JSON.stringify(['environments', 'demo']), [devEnv]);
    vi.mocked(getCollectionSettings).mockResolvedValueOnce({
      variables: [
        { key: 'K', value: 'collection-secret', initialValue: '', enabled: true, secret: true },
      ],
      headers: [],
    } as never);
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual(['collection-secret', 'tok-123456']);
  });

  it('flags the secret set as uncertain when the environment is not cached', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'a.yml', request());
    expect(resolved.secretValues).toEqual([]);
    expect(resolved.secretsUncertain).toBe(true);
  });
});
```

- [ ] **Step 3: Run both tests and see them fail**

Run: `yarn test console-store.masked execute-request.secrets`
Expected: FAIL. The store test fails on types or on missing fields, and the secrets test fails because `secretValues` is undefined.

- [ ] **Step 4: Add the entry fields**

In `src/stores/console-store.ts`, add the import as the first line:

```ts
import type { HttpExchangeText } from '@/lib/console-redaction';
```

and inside `HttpConsoleEntry`, after `requestName?: string;`:

```ts
  /** Masked copy fixed at send time. Absent for rows that are not masked, such as Flow rows. */
  masked?: HttpExchangeText;
  /** True when the secret set could not be built for this row. */
  uncertain?: boolean;
```

- [ ] **Step 5: Add the secret set to the resolved fields**

In `src/lib/execute-request.ts`:

1. Add `import { collectSecretValues } from '@/lib/console-redaction';` next to the other `@/lib/...` imports (keep Biome's import order), and add `Variable` to the existing type import from `@/lib/tauri-api` (`import type { Environment, RequestOptions } from '@/lib/tauri-api';` becomes `import type { Environment, RequestOptions, Variable } from '@/lib/tauri-api';`).
2. In `ResolvedRequestFields`, after `graphql?: ResolvedGraphQl;`, add:

```ts
  /** Secret values of this send, for the Console masks. */
  secretValues?: string[];
  /** True when the environments could not be read from the cache. */
  secretsUncertain?: boolean;
```

3. Add this function above `resolveRequestFieldsForPath`:

```ts
// Secret variables of the active environments, for the Console masks. An environment that
// should be there but is not in the query cache makes the result uncertain instead of empty.
function getSecretScope(collection?: string): { variables: Variable[]; uncertain: boolean } {
  const variables: Variable[] = [];
  let uncertain = false;
  const { activeEnvId, activeCollection } = useEnvStore.getState();
  const source = collection ?? activeCollection;
  if (activeEnvId && source) {
    const envs = getQueryClient().getQueryData<Environment[]>(environmentKeys.collection(source));
    const env = envs?.find((e) => e.name === activeEnvId);
    if (env) variables.push(...env.variables);
    else uncertain = true;
  }
  const globalEnvName =
    getQueryClient().getQueryData<string | null>(environmentKeys.globalName) ?? null;
  if (globalEnvName) {
    const env = getQueryClient().getQueryData<Environment>(environmentKeys.global(globalEnvName));
    if (env) variables.push(...env.variables);
    else uncertain = true;
  }
  return { variables, uncertain };
}
```

4. In `resolveRequestFieldsForPath`, right after the `const ctx = buildVariableContext({...});` statement, add:

```ts
  const secretScope = getSecretScope(collection);
  const secretValues = collectSecretValues([
    secretScope.variables,
    collectionVars,
    folderVars,
    requestVars,
  ]);
```

and in the returned object, after `graphql: resolvedGraphql,`, add:

```ts
    secretValues,
    secretsUncertain: secretScope.uncertain,
```

- [ ] **Step 6: Run the tests and see them pass**

Run: `yarn test console-store.masked execute-request`
Expected: PASS, including the existing `execute-request*.test.ts` files.

- [ ] **Step 7: Check types and lint**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: no errors.

---

### Task 3: Wire the masks into `sendRequest`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/execute-request.ts` (`sendRequest`: the destructuring at lines 645 to 657, the success-path `addHttpEntry` at about line 757, the error-path `addHttpEntry` at about line 815)

**Interfaces:**
- Consumes: `buildMaskOptions`, `maskHttpExchange` from Task 1; `secretValues` and `secretsUncertain` from Task 2.
- Produces: every Console row written by `sendRequest` carries `masked` and `uncertain`.

- [ ] **Step 1: Import the helpers**

Change the import added in Task 2 to:

```ts
import { buildMaskOptions, collectSecretValues, maskHttpExchange } from '@/lib/console-redaction';
```

- [ ] **Step 2: Read the secret set in the destructuring**

In `sendRequest`, extend the destructuring of `await resolveRequestFields(tabId, effectiveRequest, true)` with two names, after `graphql: resolvedGraphql,`:

```ts
    secretValues: resolvedSecretValues,
    secretsUncertain,
```

and add this line right after the `requestGuardPolicy` line:

```ts
  const maskOptions = buildMaskOptions(
    resolvedAuth,
    resolvedSecretValues ?? [],
    secretsUncertain ?? false,
  );
```

- [ ] **Step 3: Mask the success-path row**

Replace the existing `useConsoleStore.getState().addHttpEntry({ ... });` call that follows the `consoleRequestHeaders` block with:

```ts
    const consoleUrl = applyPathParams(resolvedUrl, resolvedPathParams);
    const consoleRequestBody = isGraphQl
      ? JSON.stringify({
          query: resolvedGraphql?.query,
          operationName: effectiveRequest.graphql?.operationName,
          variables: resolvedGraphql?.variables,
        })
      : (resolvedBody?.content ?? '');
    const consoleResponseHeaders = result.headers.map((h) => ({ key: h.key, value: h.value }));
    const consoleResponseBody = result.isBinary
      ? `(binary, ${result.sizeBytes} bytes)`
      : result.body;
    const redaction = maskHttpExchange(
      {
        url: consoleUrl,
        requestHeaders: consoleRequestHeaders,
        requestBody: consoleRequestBody,
        responseHeaders: consoleResponseHeaders,
        responseBody: consoleResponseBody,
      },
      maskOptions,
    );
    useConsoleStore.getState().addHttpEntry({
      method: effectiveRequest.method,
      url: consoleUrl,
      status: result.status,
      statusText: result.statusText,
      durationMs: result.durationMs,
      sizeBytes: result.sizeBytes,
      requestHeaders: consoleRequestHeaders,
      requestBody: consoleRequestBody,
      responseHeaders: consoleResponseHeaders,
      responseBody: consoleResponseBody,
      masked: redaction.masked,
      uncertain: redaction.uncertain,
    });
```

- [ ] **Step 4: Mask the error-path row**

In the `catch` block, replace the `addHttpEntry` call with:

```ts
    const errorUrl = applyPathParams(resolvedUrl, resolvedPathParams);
    const errorHeaders = effectiveHeaders.map((h) => ({ key: h.key, value: h.value }));
    const errorBody = resolvedBody?.content ?? '';
    const errorRedaction = maskHttpExchange(
      {
        url: errorUrl,
        requestHeaders: errorHeaders,
        requestBody: errorBody,
        responseHeaders: [],
        responseBody: msg,
      },
      maskOptions,
    );
    useConsoleStore.getState().addHttpEntry({
      method: effectiveRequest.method,
      url: errorUrl,
      status: 0,
      statusText: 'Error',
      durationMs: 0,
      sizeBytes: msg.length,
      requestHeaders: errorHeaders,
      requestBody: errorBody,
      responseHeaders: [],
      responseBody: msg,
      masked: errorRedaction.masked,
      uncertain: errorRedaction.uncertain,
    });
```

- [ ] **Step 5: Run the checks**

Run: `yarn tsc --noEmit`, `yarn check`, `yarn test execute-request console`
Expected: all pass. The wiring has no unit test of its own because `sendRequest` needs a live tab tree; Task 1 tests the helper and `buildMaskOptions`, and the manual check in plan 2 covers the whole path.

- [ ] **Step 6: Confirm that no other caller was touched**

Run: `git diff --stat`
Expected: only `src/lib/console-redaction.ts`, `src/lib/__tests__/console-redaction.test.ts`, `src/stores/console-store.ts`, `src/stores/__tests__/console-store.masked.test.ts`, `src/lib/execute-request.ts` and `src/lib/__tests__/execute-request.secrets.test.ts` changed. `FlowPane.tsx` must not appear.

Next plan: docs/superpowers/plans/2026-10-15-console-mask-secrets/plan-02-console-mask-secrets.md
