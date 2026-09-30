# Environment Client Certificates, Plan D: Certificates UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each environment a third **Certificates** tab in `EnvironmentDialog`, next to Variables and External Secrets. A user can add, edit, reorder and remove client certificates (PEM or PKCS12). Each piece of material comes from a file (with a native Browse button) or from a RocketVault secret reference picked from the environment's External Secrets bindings. Save validates the rules from spec section 4 and uses the dialog's single Save.

**Architecture:** Three small additive changes, frontend only. D1 widens the `Environment` type, adds the `ClientCertificate` type and two pure helpers (`vaultSecretOptions`, `validateClientCertificates`), and pins that a save keeps every field the UI does not edit. D2 adds `CertificatesTab.tsx`, a sibling of `ExternalSecretsTab.tsx` with the same `onChange(idx, patch)` / `onAdd` / `onRemove` / `onSave` / `isDirty` / `saveState` contract, plus `onMove` for reordering (the first matching certificate wins, so order matters). D3 wires it into the dialog by mirroring the external-secrets handlers 1:1 and runs `validateClientCertificates` inside `handleSave`. Plan B2 has already made the backend accept the new fields, so the TypeScript types match the persisted model.

**Tech Stack:** React, TypeScript, shadcn/ui, lucide-react, `@tauri-apps/plugin-dialog`, Vitest, @testing-library/react, @testing-library/user-event, Biome.

**Spec:** `docs/superpowers/specs/2026-10-01-environment-client-certificates-design.md` (sections 4, 9, 11, 12, 13). Plan index and shared interface contract: `docs/superpowers/plans/environment-client-certificates/00-plan-index.md`.

**Plan D of 3 (B, C, D).** Depends on Plan B2 for the types only. It may run beside B3 and C.

---

## Global Constraints (every task includes these)

- Values fetched from RocketVault are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references (`alias.secretName`) persist.
- Key bytes and passphrases held at runtime are `zeroize::Zeroizing` from resolution to use. The resolved certificate type is not `Serialize`, and its `Debug` never prints bytes or passphrases.
- A reference is `alias.secretName`, the same key RocketVault values already use in `VariableContext.external_secrets`. It is never a value.
- Each piece of material (certificate, private key, PKCS12 bundle) has exactly one source: a non-empty file path, or a reference.
- A file path or reference field must not hold key text. A value starting with `-----BEGIN` is rejected on save.
- An unresolved reference fails the request or token request only when that certificate is the one selected for the URL, with no fallback to another source.
- After decryption the PEM key must start exactly with `-----BEGIN PRIVATE KEY-----` (native TLS backend requirement).
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, never on persistence structs, except where `ClientCertificate` already has it on its variants. Persisted fields stay backward compatible (optional fields or defaults).
- Rust: never `unwrap()` in production paths. Always pass `-j4` to cargo. Do not run `cargo test --workspace`. Use targeted crate tests plus `cargo check -j4 --workspace`.
- Frontend: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`), `lucide-react` icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco only for multi-line editors, never fully destructure Zustand store state at component top level. Checks: `yarn tsc --noEmit` and `yarn check`.
- Commits: conventional commits, path-scoped (`git add <paths>` then `git commit -- <paths>`, never `git add -A` or `git commit -a`), because other sessions share this working tree. Before any commit invoke the `dev-workflow-skills:1-git-commit` skill. Commit messages end with `Relates to: #21`.
- Every task that touches collection, environment or certificate data models starts with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Review Focus (the items Plan D owns, each pinned by a named test)

4. **Overlapping domains.** `*.example.com` listed before `api.example.com` wins by order, and the tab preserves and can reorder the order. Plan D owns the reorder half. Test: `reorders certificates through onMove so the match order can change` in `src/components/environments/CertificatesTab.test.tsx` (task D2). The dialog half (the moved order reaches the saved payload) is pinned by `saves a moved certificate order` in `EnvironmentDialog.test.tsx` (task D3).
5. **A reference with no matching binding, or a pasted private key, in an environment file.** Rejected on save with a message that names the field. Plan D owns the frontend mirror. Tests: `rejects a reference with no matching binding` and `rejects a pasted private key in a path or reference field` in `src/lib/certificate-validation.test.ts` (task D1), and `blocks the save when a certificate reference has no matching binding` in `EnvironmentDialog.test.tsx` (task D3).

## Frontend rules for this plan

- shadcn/ui primitives only: `Button`, `Badge`, `Select`, `ScrollArea`, `Tabs`. No raw `<button>`, `<input>`, `<select>`, `<form>`, `<dialog>`.
- Icons from `lucide-react` only. No inline SVGs.
- Domain, file paths and passphrase are variable-aware single-line fields, so they use `SingleLineEditor` (CodeMirror 6), never Monaco and never a plain `Input`. This differs from `alias` and `vault name` in `ExternalSecretsTab`, which are not `{{var}}` aware.
- Zustand: no new store reads are needed. All new state is props and the dialog's existing local `useState`.
- In tests CodeMirror does not run in jsdom, so `@/components/editor` is replaced by a plain input with `vi.mock`, exactly as in `src/components/request/__tests__/AuthEditor.test.tsx`.

## Findings from reading the real code (do not re-derive)

- `src/components/environments/EnvironmentDialog.tsx` (388 lines). `NormalizedEnvironment = Environment & { externalSecrets: ExternalSecretBinding[] }` and `normalizeEnv` are at lines 61-71. `activeDialogTab` is `useState<'variables' | 'external-secrets'>` at lines 85-87 and the `Tabs` `onValueChange` repeats the union at line 337. The tab reset on environment change is the effect at lines 155-160 (`setActiveDialogTab('variables')`, keyed on `selectedName`). `handleSave` (lines 142-153) runs `validateExternalSecretBindings` and on error toasts and switches to the External Secrets tab. `saveSettings` (lines 126-135) spreads `...selectedEnv`, so fields the UI does not edit survive a save only because of that spread. The external-secrets handlers are at lines 242-287. `variableContext` is built at lines 293-309 and is passed to `VariableTable` only.
- `src/components/environments/ExternalSecretsTab.tsx` (295 lines): the prop pattern, the footer (Add button left, Save button right, `disabled={!isDirty || saveState !== 'idle'}`, `Loader2`/`Check`/`Save` icons), `ScrollArea`, the empty state and the `key={idx}` biome-ignore comment are the models for `CertificatesTab`.
- `src/hooks/use-save-button.ts` exports `SaveButtonState = 'idle' | 'saving' | 'success'`.
- `src/lib/tauri-api.ts`: `ExternalSecretRef { name, secretId }` at lines 184-187, `ExternalSecretBinding { alias, connectionId, vaultName, secretNames }` at lines 189-194, `Environment { name, variables, externalSecrets? }` at lines 205-209.
- `src/lib/external-secrets.ts` already holds `validateExternalSecretBindings` (alias pattern `^[A-Za-z0-9_-]+$`, so an alias never contains a dot and a reference splits on the first dot). Its test sits next to it (`src/lib/external-secrets.test.ts`), so the new lib tests go next to their files too.
- `SingleLineEditor` (`src/components/editor/SingleLineEditor.tsx`) props: `value`, `onChange`, `placeholder`, `className`, `disabled`, `variableContext?: Map<string, VariableScopeEntry>`, `isSecret`, `'aria-label'`. It is exported from `@/components/editor`.
- Native picker: `import { open } from '@tauri-apps/plugin-dialog'` and `await open({ multiple: false, title })`, which returns `string | null` for a single file (`BodyEditor.tsx` line 42, `GitCredentialsDialog.tsx` line 100 as `openFilePicker`). Tests mock it with `vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))` (`BodyEditor.test.tsx` line 12).
- Radix `Select` in jsdom needs `Element.prototype.hasPointerCapture` and `scrollIntoView` polyfills. `src/components/request/__tests__/AgentChatPanel.test.tsx` lines 10-17 shows them, and `InlineSourceEditor.test.tsx` opens a Select with `getByRole('combobox', { name })` then `findByRole('option', { name })`.
- `src/components/workspace/WorkspaceEnvironmentsTab.tsx` saves with `{ ...env, variables: editingVars }` (line 36), so it already keeps certificates. It is not changed (see the end of task D3).

## Contract notes and deviations (read before starting)

1. **`onChange` patch type.** The contract says `onChange: (idx, patch: Partial<ClientCertificate>)`. `Partial` of a union is a union of partials, so the dialog handler merges with `{ ...certificates[idx], ...patch } as ClientCertificate`. This is the only cast.
2. **Added optional prop.** `CertificatesTabProps` gains `variableContext?: Map<string, VariableScopeEntry>`, the same optional prop `VariableTable` has, so domain, path and passphrase fields highlight `{{var}}` the way the Variables tab does. It is additive and optional. The contract names are otherwise unchanged.
3. **Added export.** `src/lib/certificate-validation.ts` also exports `isLiteralPassphrase(passphrase)`, so the warning in the tab and the warning from `validateClientCertificates` use one rule (DRY). The contract exports are unchanged.
4. **`VaultSecretOption.label`** equals `value` (`alias.secretName`). A distinct label is not needed yet.
5. **How the UI knows a piece's source.** A piece is a vault piece when its secret field is a string (even `''`), and a file piece when the secret field is `undefined`. Switching to Vault sets the file path to `''` and the secret to `''`. Switching to File sets the secret to `undefined`. An empty selection is caught by validation ("needs a file path or a vault secret").

---

## Task D1: Types, vault-secret options, validation, preservation test

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Model:** sonnet.

**Files:**
- Modify: `src/lib/tauri-api.ts` (lines 189-194 stay as they are; replace `Environment` at lines 205-209 and add `ClientCertificate` above it)
- Create: `src/lib/vault-secret-options.ts`
- Create: `src/lib/vault-secret-options.test.ts`
- Create: `src/lib/certificate-validation.ts`
- Create: `src/lib/certificate-validation.test.ts`
- Modify: `src/components/environments/EnvironmentDialog.test.tsx` (append one `describe` after line 124; imports at lines 3-10 are unchanged)

**Interfaces:**
- Consumes: `ExternalSecretBinding`, `ExternalSecretRef` (existing).
- Produces (contract names): `ClientCertificate`, widened `Environment`, `VaultSecretOption`, `vaultSecretOptions(bindings)`, `CertificateIssues`, `validateClientCertificates(certs, bindings)`. Plus `isLiteralPassphrase(passphrase)` (contract note 3).

- [ ] **Step 1: Write the preservation test**

Append to `src/components/environments/EnvironmentDialog.test.tsx` (after the last `describe`):

```typescript
describe('EnvironmentDialog preserves fields it does not edit', () => {
  const fullEnv: Environment = {
    name: 'prod',
    variables: [{ key: 'HOST', value: 'https://api.example.com', enabled: true, secret: false }],
    externalSecrets: [],
    clientCertificates: [
      {
        type: 'pem',
        domain: 'api.example.com',
        certificateFilePath: 'certs/client.pem',
        privateKeyFilePath: 'certs/client.key',
        passphrase: '{{vault.keyPass}}',
      },
      { type: 'pkcs12', domain: '*.internal.example.com', pkcs12FilePath: 'certs/client.p12' },
    ],
    extends: 'base',
    dotEnvFilePath: '.env.prod',
    color: '#ff0000',
    description: { content: 'Production', type: 'text/markdown' },
  };

  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([fullEnv]);
    vi.mocked(tauriApi.saveEnvironment).mockReset().mockResolvedValue(undefined);
  });

  it('saves clientCertificates, extends, dotEnvFilePath, color and description unchanged', async () => {
    renderDialog();
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, savedEnv] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(savedEnv.variables[0].key).toBe('HOST2');
    expect(savedEnv.clientCertificates).toEqual(fullEnv.clientCertificates);
    expect(savedEnv.extends).toBe('base');
    expect(savedEnv.dotEnvFilePath).toBe('.env.prod');
    expect(savedEnv.color).toBe('#ff0000');
    expect(savedEnv.description).toEqual({ content: 'Production', type: 'text/markdown' });
  });
});
```

- [ ] **Step 2: Run the checks and confirm the expected failure**

Run: `yarn tsc --noEmit`
Expected: FAIL with errors in `EnvironmentDialog.test.tsx` such as `Object literal may only specify known properties, and 'clientCertificates' does not exist in type 'Environment'` (also for `extends`, `dotEnvFilePath`, `color`, `description`, and `Property 'clientCertificates' does not exist on type 'Environment'` on the assertions).

Run: `yarn vitest run src/components/environments/EnvironmentDialog.test.tsx`
Expected: PASS. Vitest does not type-check, and the save already spreads `...selectedEnv`. This test is a regression guard that pins today's behaviour (the spec says these fields survive only because of that spread), so the failing signal for this step is the `tsc` failure above.

- [ ] **Step 3: Widen the types**

In `src/lib/tauri-api.ts`, replace

```typescript
export interface Environment {
  name: string;
  variables: Variable[];
  externalSecrets?: ExternalSecretBinding[];
}
```

with

```typescript
// Persisted client certificate. A piece has one source: a file path or a
// vault reference (`alias.secretName`). A reference is never a value.
export type ClientCertificate =
  | {
      type: 'pem';
      domain: string;
      certificateFilePath?: string;
      privateKeyFilePath?: string;
      certificateSecret?: string;
      privateKeySecret?: string;
      passphrase?: string;
    }
  | {
      type: 'pkcs12';
      domain: string;
      pkcs12FilePath?: string;
      pkcs12Secret?: string;
      passphrase?: string;
    };

export interface Environment {
  name: string;
  variables: Variable[];
  externalSecrets?: ExternalSecretBinding[];
  clientCertificates?: ClientCertificate[];
  extends?: string;
  dotEnvFilePath?: string;
  color?: string;
  description?: unknown;
}
```

- [ ] **Step 4: Confirm the types compile**

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn vitest run src/components/environments/EnvironmentDialog.test.tsx`
Expected: PASS (all tests in the file).

- [ ] **Step 5: Write the failing test for `vaultSecretOptions`**

Create `src/lib/vault-secret-options.test.ts`:

```typescript
// src/lib/vault-secret-options.test.ts

import { describe, expect, it } from 'vitest';
import type { ExternalSecretBinding } from '@/lib/tauri-api';
import { vaultSecretOptions } from '@/lib/vault-secret-options';

function binding(alias: string, names: string[]): ExternalSecretBinding {
  return {
    alias,
    connectionId: 'conn-1',
    vaultName: 'prod-vault',
    secretNames: names.map((name, i) => ({ name, secretId: `${alias}-${i}` })),
  };
}

describe('vaultSecretOptions', () => {
  it('lists every bound secret as alias.secretName in binding order', () => {
    expect(
      vaultSecretOptions([binding('vault', ['clientCert', 'clientKey']), binding('other', ['pw'])]),
    ).toEqual([
      { value: 'vault.clientCert', label: 'vault.clientCert' },
      { value: 'vault.clientKey', label: 'vault.clientKey' },
      { value: 'other.pw', label: 'other.pw' },
    ]);
  });

  it('skips a binding with no alias and a binding with no fetched secrets', () => {
    expect(vaultSecretOptions([binding('', ['a']), binding('vault', [])])).toEqual([]);
  });

  it('returns an empty list for no bindings', () => {
    expect(vaultSecretOptions([])).toEqual([]);
  });
});
```

- [ ] **Step 6: Run it and confirm it fails**

Run: `yarn vitest run src/lib/vault-secret-options.test.ts`
Expected: FAIL with `Failed to resolve import "@/lib/vault-secret-options"`.

- [ ] **Step 7: Implement `vaultSecretOptions`**

Create `src/lib/vault-secret-options.ts`:

```typescript
// src/lib/vault-secret-options.ts

import type { ExternalSecretBinding } from '@/lib/tauri-api';

export interface VaultSecretOption {
  // The reference stored in the environment file: `${alias}.${secretName}`.
  value: string;
  label: string;
}

// Lists the secrets an environment can reference, from its External Secrets bindings.
// Only fetched secret names appear, never values.
export function vaultSecretOptions(bindings: ExternalSecretBinding[]): VaultSecretOption[] {
  const options: VaultSecretOption[] = [];
  for (const binding of bindings) {
    if (!binding.alias) continue;
    for (const ref of binding.secretNames) {
      const value = `${binding.alias}.${ref.name}`;
      options.push({ value, label: value });
    }
  }
  return options;
}
```

- [ ] **Step 8: Run it and confirm it passes**

Run: `yarn vitest run src/lib/vault-secret-options.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 9: Write the failing tests for `validateClientCertificates`**

Create `src/lib/certificate-validation.test.ts`:

```typescript
// src/lib/certificate-validation.test.ts

import { describe, expect, it } from 'vitest';
import { isLiteralPassphrase, validateClientCertificates } from '@/lib/certificate-validation';
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';

const bindings: ExternalSecretBinding[] = [
  {
    alias: 'vault',
    connectionId: 'conn-1',
    vaultName: 'prod-vault',
    secretNames: [
      { name: 'clientCertPem', secretId: '1' },
      { name: 'clientKeyPem', secretId: '2' },
      { name: 'bundleB64', secretId: '3' },
    ],
  },
];

const validPem: ClientCertificate = {
  type: 'pem',
  domain: 'api.example.com',
  certificateFilePath: 'certs/client.pem',
  privateKeyFilePath: '/etc/ssl/client.key',
};

describe('validateClientCertificates', () => {
  it('accepts file sources, vault sources and a placeholder passphrase', () => {
    const certs: ClientCertificate[] = [
      { ...validPem, passphrase: '{{vault.clientKeyPass}}' },
      {
        type: 'pem',
        domain: '*.example.com',
        certificateSecret: 'vault.clientCertPem',
        privateKeySecret: 'vault.clientKeyPem',
      },
      { type: 'pkcs12', domain: 'host:8443', pkcs12Secret: 'vault.bundleB64' },
    ];
    expect(validateClientCertificates(certs, bindings)).toEqual({ errors: [], warnings: [] });
  });

  it('accepts absolute, home and variable-prefixed paths', () => {
    const certs: ClientCertificate[] = [
      { type: 'pkcs12', domain: 'a.com', pkcs12FilePath: '/abs/../fine.p12' },
      { type: 'pkcs12', domain: 'b.com', pkcs12FilePath: '~/certs/b.p12' },
      { type: 'pkcs12', domain: 'c.com', pkcs12FilePath: '{{certDir}}/c.p12' },
    ];
    expect(validateClientCertificates(certs, bindings).errors).toEqual([]);
  });

  it('rejects an empty domain', () => {
    const { errors } = validateClientCertificates([{ ...validPem, domain: '  ' }], bindings);
    expect(errors).toEqual(['Certificate 1: domain is required.']);
  });

  it('rejects a piece with no source and a piece with both sources', () => {
    const certs: ClientCertificate[] = [
      { type: 'pem', domain: 'a.com', certificateFilePath: 'a.pem' },
      {
        type: 'pkcs12',
        domain: 'b.com',
        pkcs12FilePath: 'b.p12',
        pkcs12Secret: 'vault.bundleB64',
      },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): Private key needs a file path or a vault secret.',
      'Certificate 2 (b.com): PKCS12 bundle must have either a file path or a vault secret, not both.',
    ]);
  });

  it('treats an empty vault selection as no source', () => {
    const { errors } = validateClientCertificates(
      [{ type: 'pkcs12', domain: 'a.com', pkcs12FilePath: '', pkcs12Secret: '' }],
      bindings,
    );
    expect(errors).toEqual(['Certificate 1 (a.com): PKCS12 bundle needs a file path or a vault secret.']);
  });

  it('rejects a reference with no matching binding', () => {
    const certs: ClientCertificate[] = [
      { type: 'pkcs12', domain: 'a.com', pkcs12Secret: 'nope.bundleB64' },
      { type: 'pkcs12', domain: 'b.com', pkcs12Secret: 'vault.missing' },
      { type: 'pkcs12', domain: 'c.com', pkcs12Secret: 'noDot' },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): PKCS12 bundle secret "nope.bundleB64" has no External Secrets binding with alias "nope".',
      'Certificate 2 (b.com): PKCS12 bundle secret "vault.missing" is not one of the fetched secrets for alias "vault". Fetch the secrets on the External Secrets tab.',
      'Certificate 3 (c.com): PKCS12 bundle secret "noDot" must look like alias.secretName.',
    ]);
  });

  it('rejects a pasted private key in a path or reference field', () => {
    const certs: ClientCertificate[] = [
      {
        type: 'pem',
        domain: 'a.com',
        certificateFilePath: 'a.pem',
        privateKeyFilePath: '-----BEGIN PRIVATE KEY-----\nMIIE',
      },
      { type: 'pkcs12', domain: 'b.com', pkcs12Secret: '  -----BEGIN CERTIFICATE-----' },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): Private key file path must be a file path or a vault secret reference, not key text.',
      'Certificate 2 (b.com): PKCS12 bundle secret must be a file path or a vault secret reference, not key text.',
    ]);
  });

  it('rejects a relative path that contains ..', () => {
    const certs: ClientCertificate[] = [
      { type: 'pkcs12', domain: 'a.com', pkcs12FilePath: '../outside/a.p12' },
      { type: 'pkcs12', domain: 'b.com', pkcs12FilePath: 'certs\\..\\b.p12' },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): PKCS12 bundle file path must not contain "..".',
      'Certificate 2 (b.com): PKCS12 bundle file path must not contain "..".',
    ]);
  });

  it('warns, but does not error, on a literal passphrase', () => {
    const { errors, warnings } = validateClientCertificates(
      [{ ...validPem, passphrase: 'hunter2' }],
      bindings,
    );
    expect(errors).toEqual([]);
    expect(warnings).toEqual([
      'Certificate 1 (api.example.com): the passphrase is a literal and will be saved in the environment file. Use a vault secret placeholder such as {{vault.NAME}}.',
    ]);
  });

  it('does not warn for an empty passphrase or a placeholder', () => {
    expect(validateClientCertificates([{ ...validPem, passphrase: '' }], bindings).warnings).toEqual([]);
    expect(
      validateClientCertificates([{ ...validPem, passphrase: 'pre-{{vault.p}}' }], bindings).warnings,
    ).toEqual([]);
  });
});

describe('isLiteralPassphrase', () => {
  it('is true only for a non-empty value with no {{...}} placeholder', () => {
    expect(isLiteralPassphrase(undefined)).toBe(false);
    expect(isLiteralPassphrase('')).toBe(false);
    expect(isLiteralPassphrase('{{vault.pass}}')).toBe(false);
    expect(isLiteralPassphrase('hunter2')).toBe(true);
  });
});
```

- [ ] **Step 10: Run it and confirm it fails**

Run: `yarn vitest run src/lib/certificate-validation.test.ts`
Expected: FAIL with `Failed to resolve import "@/lib/certificate-validation"`.

- [ ] **Step 11: Implement `validateClientCertificates`**

Create `src/lib/certificate-validation.ts`:

```typescript
// src/lib/certificate-validation.ts

import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';

export interface CertificateIssues {
  errors: string[];
  warnings: string[];
}

const KEY_TEXT_PREFIX = '-----BEGIN';
const PLACEHOLDER = /\{\{[^}]+\}\}/;

// True for a passphrase that would be written to the file as typed.
export function isLiteralPassphrase(passphrase: string | undefined): boolean {
  return !!passphrase && !PLACEHOLDER.test(passphrase);
}

interface Piece {
  label: string;
  filePath: string;
  secret: string;
}

function piecesOf(cert: ClientCertificate): Piece[] {
  if (cert.type === 'pem') {
    return [
      {
        label: 'Certificate',
        filePath: cert.certificateFilePath ?? '',
        secret: cert.certificateSecret ?? '',
      },
      {
        label: 'Private key',
        filePath: cert.privateKeyFilePath ?? '',
        secret: cert.privateKeySecret ?? '',
      },
    ];
  }
  return [
    { label: 'PKCS12 bundle', filePath: cert.pkcs12FilePath ?? '', secret: cert.pkcs12Secret ?? '' },
  ];
}

function isKeyText(value: string): boolean {
  return value.trimStart().startsWith(KEY_TEXT_PREFIX);
}

// Absolute, home and variable-prefixed paths are not relative. A variable
// prefix is unknown until it resolves, so it is not checked here.
function isRelativePath(path: string): boolean {
  if (path.startsWith('{{')) return false;
  if (path.startsWith('/') || path.startsWith('\\')) return false;
  if (path === '~' || path.startsWith('~/')) return false;
  return !/^[A-Za-z]:[\\/]/.test(path);
}

function hasParentSegment(path: string): boolean {
  return path.split(/[\\/]/).includes('..');
}

// Returns why a reference is unusable, or null when it matches a binding and one of its names.
function referenceProblem(reference: string, bindings: ExternalSecretBinding[]): string | null {
  const dot = reference.indexOf('.');
  if (dot <= 0 || dot === reference.length - 1) return 'must look like alias.secretName';
  const alias = reference.slice(0, dot);
  const name = reference.slice(dot + 1);
  const binding = bindings.find((b) => b.alias === alias);
  if (!binding) return `has no External Secrets binding with alias "${alias}"`;
  if (!binding.secretNames.some((ref) => ref.name === name)) {
    return `is not one of the fetched secrets for alias "${alias}". Fetch the secrets on the External Secrets tab`;
  }
  return null;
}

// Mirrors `validate_client_certificates` in rocket-environment. Errors block the save.
// A literal passphrase is only a warning.
export function validateClientCertificates(
  certs: ClientCertificate[],
  bindings: ExternalSecretBinding[],
): CertificateIssues {
  const errors: string[] = [];
  const warnings: string[] = [];

  for (const [idx, cert] of certs.entries()) {
    const domain = cert.domain.trim();
    const who = domain ? `Certificate ${idx + 1} (${domain})` : `Certificate ${idx + 1}`;
    if (!domain) errors.push(`${who}: domain is required.`);

    for (const piece of piecesOf(cert)) {
      const hasFile = piece.filePath.trim() !== '';
      const hasSecret = piece.secret.trim() !== '';
      if (hasFile && hasSecret) {
        errors.push(`${who}: ${piece.label} must have either a file path or a vault secret, not both.`);
        continue;
      }
      if (!hasFile && !hasSecret) {
        errors.push(`${who}: ${piece.label} needs a file path or a vault secret.`);
        continue;
      }
      if (hasFile) {
        if (isKeyText(piece.filePath)) {
          errors.push(
            `${who}: ${piece.label} file path must be a file path or a vault secret reference, not key text.`,
          );
        } else if (isRelativePath(piece.filePath) && hasParentSegment(piece.filePath)) {
          errors.push(`${who}: ${piece.label} file path must not contain "..".`);
        }
      } else if (isKeyText(piece.secret)) {
        errors.push(
          `${who}: ${piece.label} secret must be a file path or a vault secret reference, not key text.`,
        );
      } else {
        const problem = referenceProblem(piece.secret, bindings);
        if (problem) errors.push(`${who}: ${piece.label} secret "${piece.secret}" ${problem}.`);
      }
    }

    if (isLiteralPassphrase(cert.passphrase)) {
      warnings.push(
        `${who}: the passphrase is a literal and will be saved in the environment file. Use a vault secret placeholder such as {{vault.NAME}}.`,
      );
    }
  }

  return { errors, warnings };
}
```

The missing-name message ends `... Fetch the secrets on the External Secrets tab.` because the final `.` is appended after `problem`. If a string in the test does not match, fix the code, not the test.

- [ ] **Step 12: Run it and confirm it passes**

Run: `yarn vitest run src/lib/certificate-validation.test.ts src/lib/vault-secret-options.test.ts`
Expected: PASS.

- [ ] **Step 13: Run the checks**

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS. If Biome reports formatting only, run `yarn format` and re-run `yarn check`.

- [ ] **Step 14: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage only these paths, then commit with the same paths:

```bash
git add src/lib/tauri-api.ts src/lib/vault-secret-options.ts src/lib/vault-secret-options.test.ts src/lib/certificate-validation.ts src/lib/certificate-validation.test.ts src/components/environments/EnvironmentDialog.test.tsx
git commit -- src/lib/tauri-api.ts src/lib/vault-secret-options.ts src/lib/vault-secret-options.test.ts src/lib/certificate-validation.ts src/lib/certificate-validation.test.ts src/components/environments/EnvironmentDialog.test.tsx
```

Suggested message: `feat(environments): add client certificate types and validation helpers`, with a body that says the save preservation test pins the widened fields, and a last line `Relates to: #21`.

---

## Task D2: `CertificatesTab` component

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Model:** sonnet.

**Files:**
- Create: `src/components/environments/CertificatesTab.tsx`
- Create: `src/components/environments/CertificatesTab.test.tsx`

**Interfaces:**
- Consumes: `ClientCertificate`, `ExternalSecretBinding` (`@/lib/tauri-api`), `vaultSecretOptions`, `VaultSecretOption` (`@/lib/vault-secret-options`), `isLiteralPassphrase` (`@/lib/certificate-validation`), `SaveButtonState` (`@/hooks/use-save-button`), `SingleLineEditor` (`@/components/editor`), `open` (`@tauri-apps/plugin-dialog`), `VariableScopeEntry` (`@/lib/url-variables`).
- Produces (contract): `CertificatesTabProps` and `CertificatesTab`. Contract note 2 adds the optional `variableContext`.

Accessible names used by the tests and by task D3 (n is the 1-based row number): `Domain for certificate n`, `Certificate source for certificate n` (and `Private key source ...`, `PKCS12 bundle source ...`), `Certificate file path for certificate n`, `Browse for certificate file for certificate n`, `Certificate vault secret for certificate n`, `Passphrase for certificate n`, `Insert vault secret into passphrase for certificate n`, `Move certificate n up`, `Move certificate n down`, `Delete certificate n`, buttons `Add PEM` and `Add PKCS12`, and `Save`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/environments/CertificatesTab.test.tsx`:

```typescript
// src/components/environments/CertificatesTab.test.tsx

import { open } from '@tauri-apps/plugin-dialog';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CertificatesTab } from '@/components/environments/CertificatesTab';
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    placeholder,
    isSecret,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
    isSecret?: boolean;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={label}
      placeholder={placeholder}
      data-secret={isSecret ? 'true' : 'false'}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op for test polyfill.
  };
}

const bindings: ExternalSecretBinding[] = [
  {
    alias: 'vault',
    connectionId: 'conn-1',
    vaultName: 'prod-vault',
    secretNames: [
      { name: 'clientCertPem', secretId: '1' },
      { name: 'clientKeyPass', secretId: '2' },
    ],
  },
];

const pem: ClientCertificate = {
  type: 'pem',
  domain: 'api.example.com',
  certificateFilePath: 'certs/a.pem',
  privateKeyFilePath: 'certs/a.key',
};

const pkcs12: ClientCertificate = {
  type: 'pkcs12',
  domain: '*.example.com',
  pkcs12FilePath: 'certs/b.p12',
};

function renderTab(
  certificates: ClientCertificate[],
  overrides: Partial<{ isDirty: boolean; onSave: () => void }> = {},
) {
  const handlers = {
    onChange: vi.fn(),
    onAdd: vi.fn(),
    onRemove: vi.fn(),
    onMove: vi.fn(),
    onSave: overrides.onSave ?? vi.fn(),
  };
  render(
    <CertificatesTab
      certificates={certificates}
      bindings={bindings}
      {...handlers}
      isDirty={overrides.isDirty ?? false}
      saveState='idle'
    />,
  );
  return handlers;
}

describe('CertificatesTab', () => {
  beforeEach(() => {
    vi.mocked(open).mockReset();
  });

  it('shows an empty state and the add buttons when there are no certificates', () => {
    renderTab([]);
    expect(screen.getByText('No client certificates')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add PEM' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add PKCS12' })).toBeInTheDocument();
  });

  it('shows a type badge, the domain and the hints for each row', () => {
    renderTab([pem, pkcs12]);
    expect(screen.getByText('PEM')).toBeInTheDocument();
    expect(screen.getByText('PKCS12')).toBeInTheDocument();
    expect(screen.getByLabelText('Domain for certificate 1')).toHaveValue('api.example.com');
    expect(screen.getByLabelText('Domain for certificate 2')).toHaveValue('*.example.com');
    expect(screen.getByText(/wildcard/i)).toBeInTheDocument();
    expect(screen.getByText(/Relative paths start at the collection folder/)).toBeInTheDocument();
    expect(screen.getByText(/Encrypted PEM keys need their passphrase/)).toBeInTheDocument();
  });

  it('adds a PEM and a PKCS12 certificate', async () => {
    const { onAdd } = renderTab([pem]);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Add PEM' }));
    await user.click(screen.getByRole('button', { name: 'Add PKCS12' }));
    expect(onAdd).toHaveBeenNthCalledWith(1, 'pem');
    expect(onAdd).toHaveBeenNthCalledWith(2, 'pkcs12');
  });

  it('edits the domain and a file path', () => {
    const { onChange } = renderTab([pem]);
    fireEvent.change(screen.getByLabelText('Domain for certificate 1'), {
      target: { value: 'b.example.com:8443' },
    });
    expect(onChange).toHaveBeenLastCalledWith(0, { domain: 'b.example.com:8443' });
    fireEvent.change(screen.getByLabelText('Private key file path for certificate 1'), {
      target: { value: 'certs/new.key' },
    });
    expect(onChange).toHaveBeenLastCalledWith(0, { privateKeyFilePath: 'certs/new.key' });
  });

  it('removes the right certificate', async () => {
    const { onRemove } = renderTab([pem, pkcs12]);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Delete certificate 2' }));
    expect(onRemove).toHaveBeenCalledWith(1);
  });

  it('reorders certificates through onMove so the match order can change', async () => {
    const { onMove } = renderTab([pem, pkcs12]);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Move certificate 2 up' }));
    expect(onMove).toHaveBeenLastCalledWith(1, -1);
    await user.click(screen.getByRole('button', { name: 'Move certificate 1 down' }));
    expect(onMove).toHaveBeenLastCalledWith(0, 1);
    // The ends cannot move past the list.
    expect(screen.getByRole('button', { name: 'Move certificate 1 up' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Move certificate 2 down' })).toBeDisabled();
  });

  it('switching to a vault source clears the file path', async () => {
    const { onChange } = renderTab([pem]);
    const user = userEvent.setup();
    await user.click(screen.getByRole('combobox', { name: 'Certificate source for certificate 1' }));
    await user.click(await screen.findByRole('option', { name: 'Vault secret' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificateFilePath: '', certificateSecret: '' });
  });

  it('switching to a file source clears the secret', async () => {
    const vaultPem: ClientCertificate = {
      type: 'pem',
      domain: 'api.example.com',
      certificateSecret: 'vault.clientCertPem',
      privateKeyFilePath: 'certs/a.key',
    };
    const { onChange } = renderTab([vaultPem]);
    const user = userEvent.setup();
    await user.click(screen.getByRole('combobox', { name: 'Certificate source for certificate 1' }));
    await user.click(await screen.findByRole('option', { name: 'File' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificateSecret: undefined });
  });

  it('picks a vault secret from the bound secrets', async () => {
    const vaultPem: ClientCertificate = {
      type: 'pem',
      domain: 'api.example.com',
      certificateSecret: '',
      privateKeyFilePath: 'certs/a.key',
    };
    const { onChange } = renderTab([vaultPem]);
    const user = userEvent.setup();
    await user.click(
      screen.getByRole('combobox', { name: 'Certificate vault secret for certificate 1' }),
    );
    await user.click(await screen.findByRole('option', { name: 'vault.clientCertPem' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificateSecret: 'vault.clientCertPem' });
  });

  it('keeps a saved reference visible when it is not in the fetched names', async () => {
    const stale: ClientCertificate = { type: 'pkcs12', domain: 'a.com', pkcs12Secret: 'vault.gone' };
    renderTab([stale]);
    const trigger = screen.getByRole('combobox', {
      name: 'PKCS12 bundle vault secret for certificate 1',
    });
    expect(trigger).toHaveTextContent('vault.gone (not fetched)');
  });

  it('browses for a file with the native picker', async () => {
    vi.mocked(open).mockResolvedValue('/home/user/client.pem');
    const { onChange } = renderTab([pem]);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Browse for certificate file for certificate 1' }));
    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith(0, { certificateFilePath: '/home/user/client.pem' }),
    );
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ multiple: false }));
  });

  it('does not change anything when the picker is cancelled', async () => {
    vi.mocked(open).mockResolvedValue(null);
    const { onChange } = renderTab([pem]);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Browse for certificate file for certificate 1' }));
    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(onChange).not.toHaveBeenCalled();
  });

  it('masks the passphrase field and edits the passphrase', () => {
    const { onChange } = renderTab([{ ...pem, passphrase: 'abc' }]);
    const field = screen.getByLabelText('Passphrase for certificate 1');
    expect(field).toHaveAttribute('data-secret', 'true');
    fireEvent.change(field, { target: { value: 'abcd' } });
    expect(onChange).toHaveBeenLastCalledWith(0, { passphrase: 'abcd' });
    fireEvent.change(field, { target: { value: '' } });
    expect(onChange).toHaveBeenLastCalledWith(0, { passphrase: undefined });
  });

  it('inserts a vault placeholder into the passphrase', async () => {
    const { onChange } = renderTab([pem]);
    const user = userEvent.setup();
    await user.click(
      screen.getByRole('combobox', {
        name: 'Insert vault secret into passphrase for certificate 1',
      }),
    );
    await user.click(await screen.findByRole('option', { name: 'vault.clientKeyPass' }));
    expect(onChange).toHaveBeenCalledWith(0, { passphrase: '{{vault.clientKeyPass}}' });
  });

  it('warns about a literal passphrase and stays quiet for a placeholder or none', () => {
    renderTab([
      { ...pem, passphrase: 'hunter2' },
      { ...pkcs12, passphrase: '{{vault.clientKeyPass}}' },
    ]);
    expect(screen.getAllByText(/saved in the environment file/)).toHaveLength(1);
  });

  it('enables Save only when dirty and calls onSave', async () => {
    const onSave = vi.fn();
    renderTab([pem], { isDirty: true, onSave });
    await userEvent.setup().click(screen.getByRole('button', { name: /^save$/i }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it('disables Save when nothing changed', () => {
    renderTab([pem], { isDirty: false });
    expect(screen.getByRole('button', { name: /^save$/i })).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `yarn vitest run src/components/environments/CertificatesTab.test.tsx`
Expected: FAIL with `Failed to resolve import "@/components/environments/CertificatesTab"`.

- [ ] **Step 3: Implement the component**

Create `src/components/environments/CertificatesTab.tsx`:

```tsx
// src/components/environments/CertificatesTab.tsx

import { open } from '@tauri-apps/plugin-dialog';
import { ArrowDown, ArrowUp, Check, FolderOpen, Loader2, Plus, Save, X } from 'lucide-react';
import { useMemo } from 'react';
import { toast } from 'sonner';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { isLiteralPassphrase } from '@/lib/certificate-validation';
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { type VaultSecretOption, vaultSecretOptions } from '@/lib/vault-secret-options';

export interface CertificatesTabProps {
  certificates: ClientCertificate[];
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  onAdd: (type: 'pem' | 'pkcs12') => void;
  onRemove: (idx: number) => void;
  onMove: (idx: number, direction: -1 | 1) => void;
  onSave: () => void;
  isDirty: boolean;
  saveState: SaveButtonState;
  variableContext?: Map<string, VariableScopeEntry>;
}

type PieceSource = 'file' | 'vault';

// One piece of material (certificate, private key or PKCS12 bundle) and the
// patches that edit it. A piece is a vault piece when its secret is a string.
interface PieceSpec {
  label: string;
  filePath: string;
  secret: string | undefined;
  withFilePath: (path: string) => Partial<ClientCertificate>;
  withSecret: (secret: string) => Partial<ClientCertificate>;
  toFile: Partial<ClientCertificate>;
  toVault: Partial<ClientCertificate>;
}

function pieceSpecs(cert: ClientCertificate): PieceSpec[] {
  if (cert.type === 'pem') {
    return [
      {
        label: 'Certificate',
        filePath: cert.certificateFilePath ?? '',
        secret: cert.certificateSecret,
        withFilePath: (path) => ({ certificateFilePath: path }),
        withSecret: (secret) => ({ certificateSecret: secret }),
        toFile: { certificateSecret: undefined },
        toVault: { certificateFilePath: '', certificateSecret: '' },
      },
      {
        label: 'Private key',
        filePath: cert.privateKeyFilePath ?? '',
        secret: cert.privateKeySecret,
        withFilePath: (path) => ({ privateKeyFilePath: path }),
        withSecret: (secret) => ({ privateKeySecret: secret }),
        toFile: { privateKeySecret: undefined },
        toVault: { privateKeyFilePath: '', privateKeySecret: '' },
      },
    ];
  }
  return [
    {
      label: 'PKCS12 bundle',
      filePath: cert.pkcs12FilePath ?? '',
      secret: cert.pkcs12Secret,
      withFilePath: (path) => ({ pkcs12FilePath: path }),
      withSecret: (secret) => ({ pkcs12Secret: secret }),
      toFile: { pkcs12Secret: undefined },
      toVault: { pkcs12FilePath: '', pkcs12Secret: '' },
    },
  ];
}

export function CertificatesTab({
  certificates,
  bindings,
  onChange,
  onAdd,
  onRemove,
  onMove,
  onSave,
  isDirty,
  saveState,
  variableContext,
}: CertificatesTabProps) {
  const options = useMemo(() => vaultSecretOptions(bindings), [bindings]);

  return (
    <div className='flex-1 flex flex-col min-w-0'>
      <div className='px-3 pt-3 pb-2 border-b border-border/40 shrink-0 space-y-0.5'>
        <p className='text-[11px] text-muted-foreground'>
          Domain: use * as a wildcard, for example *.example.com, and add :port to match one port.
          The first matching certificate is used, so put specific domains before wildcards.
        </p>
        <p className='text-[11px] text-muted-foreground'>
          Relative paths start at the collection folder. Encrypted PEM keys need their passphrase.
        </p>
      </div>

      {certificates.length === 0 ? (
        <div className='flex-1 flex flex-col items-center justify-center gap-1 text-center px-6'>
          <p className='text-sm font-medium text-foreground'>No client certificates</p>
          <p className='text-xs text-muted-foreground leading-relaxed max-w-[280px]'>
            Add a PEM or PKCS12 certificate to present it to matching hosts.
          </p>
        </div>
      ) : (
        <ScrollArea className='flex-1'>
          <div className='px-3 pt-2 pb-1 space-y-3'>
            {certificates.map((cert, idx) => (
              <CertificateRow
                // biome-ignore lint/suspicious/noArrayIndexKey: rows are fully controlled and hold no local state
                key={idx}
                idx={idx}
                total={certificates.length}
                cert={cert}
                options={options}
                onChange={onChange}
                onRemove={onRemove}
                onMove={onMove}
                variableContext={variableContext}
              />
            ))}
          </div>
        </ScrollArea>
      )}

      <div className='px-3 py-2 border-t border-border/40 flex items-center justify-between shrink-0'>
        <div className='flex items-center gap-1'>
          <Button
            variant='ghost'
            size='sm'
            onClick={() => onAdd('pem')}
            className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
          >
            <Plus className='h-3.5 w-3.5' />
            Add PEM
          </Button>
          <Button
            variant='ghost'
            size='sm'
            onClick={() => onAdd('pkcs12')}
            className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
          >
            <Plus className='h-3.5 w-3.5' />
            Add PKCS12
          </Button>
        </div>
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

interface CertificateRowProps {
  idx: number;
  total: number;
  cert: ClientCertificate;
  options: VaultSecretOption[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  onRemove: (idx: number) => void;
  onMove: (idx: number, direction: -1 | 1) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

function CertificateRow({
  idx,
  total,
  cert,
  options,
  onChange,
  onRemove,
  onMove,
  variableContext,
}: CertificateRowProps) {
  const n = idx + 1;

  return (
    <div className='space-y-2 pb-3 border-b border-border/20 last:border-0'>
      <div className='flex items-center gap-1.5 min-w-0'>
        <Badge variant='secondary' className='text-[11px] shrink-0'>
          {cert.type === 'pem' ? 'PEM' : 'PKCS12'}
        </Badge>
        <div className='flex-1 min-w-0'>
          <SingleLineEditor
            aria-label={`Domain for certificate ${n}`}
            placeholder='Domain, for example *.example.com'
            value={cert.domain}
            onChange={(domain) => onChange(idx, { domain })}
            variableContext={variableContext}
            className='h-7 text-xs font-mono'
          />
        </div>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          disabled={idx === 0}
          onClick={() => onMove(idx, -1)}
          aria-label={`Move certificate ${n} up`}
        >
          <ArrowUp className='h-3.5 w-3.5 text-muted-foreground' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          disabled={idx === total - 1}
          onClick={() => onMove(idx, 1)}
          aria-label={`Move certificate ${n} down`}
        >
          <ArrowDown className='h-3.5 w-3.5 text-muted-foreground' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          onClick={() => onRemove(idx)}
          aria-label={`Delete certificate ${n}`}
        >
          <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
        </Button>
      </div>

      {pieceSpecs(cert).map((spec) => (
        <PieceField
          key={spec.label}
          certNumber={n}
          idx={idx}
          spec={spec}
          options={options}
          onChange={onChange}
          variableContext={variableContext}
        />
      ))}

      <PassphraseField
        idx={idx}
        certNumber={n}
        passphrase={cert.passphrase ?? ''}
        options={options}
        onChange={onChange}
        variableContext={variableContext}
      />
    </div>
  );
}

interface PieceFieldProps {
  idx: number;
  certNumber: number;
  spec: PieceSpec;
  options: VaultSecretOption[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

function PieceField({ idx, certNumber, spec, options, onChange, variableContext }: PieceFieldProps) {
  const source: PieceSource = spec.secret !== undefined ? 'vault' : 'file';
  const name = `${spec.label} `;
  const lower = spec.label.toLowerCase();

  // A saved reference can point at a secret that is no longer fetched. Keep it visible.
  const secretOptions =
    spec.secret && !options.some((o) => o.value === spec.secret)
      ? [...options, { value: spec.secret, label: `${spec.secret} (not fetched)` }]
      : options;

  const browse = async () => {
    try {
      const picked = await open({ multiple: false, title: `Select ${lower} file` });
      if (typeof picked === 'string' && picked) onChange(idx, spec.withFilePath(picked));
    } catch (err) {
      console.error('[CertificatesTab] file picker failed:', err);
      toast.error('Could not open the file picker');
    }
  };

  return (
    <div className='space-y-1'>
      <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        {spec.label}
      </p>
      <div className='flex items-center gap-1.5 min-w-0'>
        <Select
          value={source}
          onValueChange={(v) => {
            if (v === source) return;
            onChange(idx, v === 'vault' ? spec.toVault : spec.toFile);
          }}
        >
          <SelectTrigger
            className='h-7 w-[120px] shrink-0 text-xs'
            aria-label={`${name}source for certificate ${certNumber}`}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='file' className='text-xs'>
              File
            </SelectItem>
            <SelectItem value='vault' className='text-xs'>
              Vault secret
            </SelectItem>
          </SelectContent>
        </Select>

        {source === 'file' ? (
          <>
            <div className='flex-1 min-w-0'>
              <SingleLineEditor
                aria-label={`${name}file path for certificate ${certNumber}`}
                placeholder='Path to the file'
                value={spec.filePath}
                onChange={(path) => onChange(idx, spec.withFilePath(path))}
                variableContext={variableContext}
                className='h-7 text-xs font-mono'
              />
            </div>
            <Button
              variant='outline'
              size='sm'
              className='h-7 text-xs gap-1.5 shrink-0'
              onClick={() => void browse()}
              aria-label={`Browse for ${lower} file for certificate ${certNumber}`}
            >
              <FolderOpen className='h-3.5 w-3.5' />
              Browse
            </Button>
          </>
        ) : (
          <Select
            value={spec.secret ?? ''}
            onValueChange={(v) => onChange(idx, spec.withSecret(v))}
          >
            <SelectTrigger
              className='h-7 min-w-0 flex-1 text-xs font-mono'
              aria-label={`${name}vault secret for certificate ${certNumber}`}
            >
              <SelectValue placeholder='Select a vault secret' />
            </SelectTrigger>
            <SelectContent>
              {secretOptions.map((option) => (
                <SelectItem key={option.value} value={option.value} className='text-xs font-mono'>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>
    </div>
  );
}

interface PassphraseFieldProps {
  idx: number;
  certNumber: number;
  passphrase: string;
  options: VaultSecretOption[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

function PassphraseField({
  idx,
  certNumber,
  passphrase,
  options,
  onChange,
  variableContext,
}: PassphraseFieldProps) {
  return (
    <div className='space-y-1'>
      <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        Passphrase
      </p>
      <div className='flex items-center gap-1.5 min-w-0'>
        <div className='flex-1 min-w-0'>
          <SingleLineEditor
            aria-label={`Passphrase for certificate ${certNumber}`}
            placeholder='Optional, for example {{vault.NAME}}'
            value={passphrase}
            onChange={(value) => onChange(idx, { passphrase: value === '' ? undefined : value })}
            isSecret
            variableContext={variableContext}
            className='h-7 text-xs font-mono'
          />
        </div>
        <Select
          value=''
          onValueChange={(v) => onChange(idx, { passphrase: `{{${v}}}` })}
          disabled={options.length === 0}
        >
          <SelectTrigger
            className='h-7 w-[170px] shrink-0 text-xs'
            aria-label={`Insert vault secret into passphrase for certificate ${certNumber}`}
          >
            <SelectValue placeholder='Insert vault secret' />
          </SelectTrigger>
          <SelectContent>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value} className='text-xs font-mono'>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {isLiteralPassphrase(passphrase) && (
        <p className='text-[11px] text-amber-600 dark:text-amber-500'>
          This passphrase is saved in the environment file as typed. Use a vault secret placeholder
          such as {'{{vault.NAME}}'} instead.
        </p>
      )}
    </div>
  );
}
```

Two details the implementer must keep: (a) the warning text must contain the phrase `saved in the environment file` (the test matches it); (b) the file-path placeholder uses plain quotes, and `{'{{vault.NAME}}'}` is a JSX string expression so the braces are not parsed as JSX.

- [ ] **Step 4: Run it and confirm it passes**

Run: `yarn vitest run src/components/environments/CertificatesTab.test.tsx`
Expected: PASS (17 tests). If the `Select` interaction tests cannot open the dropdown, the polyfills at the top of the test file are the cause; do not change the component.

- [ ] **Step 5: Run the checks**

Run: `yarn tsc --noEmit`
Expected: PASS. If `Partial<ClientCertificate>` object literals in `pieceSpecs` are rejected, keep the literals and annotate only the return type (`PieceSpec[]`), do not add a cast.

Run: `yarn check`
Expected: PASS. If Biome reports formatting only, run `yarn format` and re-run `yarn check`.

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage only these paths, then commit with the same paths:

```bash
git add src/components/environments/CertificatesTab.tsx src/components/environments/CertificatesTab.test.tsx
git commit -- src/components/environments/CertificatesTab.tsx src/components/environments/CertificatesTab.test.tsx
```

Suggested message: `feat(environments): add the client certificates tab component`, ending with `Relates to: #21`.

---

## Task D3: Wire the tab into `EnvironmentDialog`

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Model:** sonnet, reviewed by the orchestrator.

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx` (imports lines 14-30, normalize lines 61-71, tab state lines 85-87, `handleSave` lines 142-153, new handlers after line 287, render lines 335-371)
- Modify: `src/components/environments/EnvironmentDialog.test.tsx` (mocks near lines 12-23, new `describe` blocks at the end)

**Interfaces:**
- Consumes: `CertificatesTab` and `CertificatesTabProps` (D2), `validateClientCertificates` (D1), `ClientCertificate` (D1).
- Produces: a `'certificates'` dialog tab and handlers `updateClientCertificate`, `addClientCertificate`, `removeClientCertificate`, `moveClientCertificate` in the dialog. `NormalizedEnvironment` always has `clientCertificates: ClientCertificate[]`.

- [ ] **Step 1: Write the failing tests**

In `src/components/environments/EnvironmentDialog.test.tsx`:

(a) Add this import between `@testing-library/user-event` and `vitest` (Biome sorts imports; `yarn check` confirms):

```typescript
import { toast } from 'sonner';
```

(b) After the existing `vi.mock('@/lib/tauri-api', ...)` block add:

```typescript
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

vi.mock('sonner', () => ({ toast: { error: vi.fn(), warning: vi.fn() } }));
```

(c) Append at the end of the file:

```typescript
describe('EnvironmentDialog certificates tab', () => {
  const vaultBinding = {
    alias: 'vault',
    connectionId: 'conn-1',
    vaultName: 'prod-vault',
    secretNames: [{ name: 'bundleB64', secretId: '1' }],
  };

  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
    vi.mocked(tauriApi.saveEnvironment).mockReset().mockResolvedValue(undefined);
    vi.mocked(toast.error).mockClear();
    vi.mocked(toast.warning).mockClear();
  });

  it('switches to the Certificates tab and back', async () => {
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.click(screen.getByRole('tab', { name: /certificates/i }));
    expect(screen.queryByLabelText('Variable key 1')).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Add PEM' })).toBeInTheDocument();

    await user.click(screen.getByRole('tab', { name: /^variables$/i }));
    expect(await screen.findByLabelText('Variable key 1')).toBeInTheDocument();
  });

  it('saves an added certificate in the payload', async () => {
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.click(screen.getByRole('tab', { name: /certificates/i }));
    await user.click(await screen.findByRole('button', { name: 'Add PEM' }));
    await user.type(screen.getByLabelText('Domain for certificate 1'), 'api.example.com');
    await user.type(screen.getByLabelText('Certificate file path for certificate 1'), 'certs/a.pem');
    await user.type(screen.getByLabelText('Private key file path for certificate 1'), 'certs/a.key');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, savedEnv] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(savedEnv.clientCertificates).toEqual([
      {
        type: 'pem',
        domain: 'api.example.com',
        certificateFilePath: 'certs/a.pem',
        privateKeyFilePath: 'certs/a.key',
      },
    ]);
  });

  it('saves a moved certificate order', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([
      {
        ...prodEnv,
        clientCertificates: [
          { type: 'pkcs12', domain: '*.example.com', pkcs12FilePath: 'wild.p12' },
          { type: 'pkcs12', domain: 'api.example.com', pkcs12FilePath: 'api.p12' },
        ],
      },
    ]);
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.click(screen.getByRole('tab', { name: /certificates/i }));
    await user.click(await screen.findByRole('button', { name: 'Move certificate 2 up' }));
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, savedEnv] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(savedEnv.clientCertificates?.map((c) => c.domain)).toEqual([
      'api.example.com',
      '*.example.com',
    ]);
  });

  it('removes a certificate', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([
      {
        ...prodEnv,
        clientCertificates: [{ type: 'pkcs12', domain: 'a.com', pkcs12FilePath: 'a.p12' }],
      },
    ]);
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.click(screen.getByRole('tab', { name: /certificates/i }));
    await user.click(await screen.findByRole('button', { name: 'Delete certificate 1' }));
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, savedEnv] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(savedEnv.clientCertificates).toEqual([]);
  });

  it('blocks the save when a certificate reference has no matching binding', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([
      {
        ...prodEnv,
        externalSecrets: [vaultBinding],
        clientCertificates: [{ type: 'pkcs12', domain: 'a.com', pkcs12Secret: 'vault.missing' }],
      },
    ]);
    renderDialog();
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveEnvironment).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('vault.missing'));
    // The dialog moves to the tab that holds the problem.
    expect(await screen.findByRole('button', { name: 'Add PEM' })).toBeInTheDocument();
  });

  it('does not block the save for a literal passphrase, only warns', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([
      {
        ...prodEnv,
        clientCertificates: [
          { type: 'pkcs12', domain: 'a.com', pkcs12FilePath: 'a.p12', passphrase: 'hunter2' },
        ],
      },
    ]);
    renderDialog();
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    expect(toast.error).not.toHaveBeenCalled();
    expect(toast.warning).toHaveBeenCalledWith(expect.stringContaining('literal'));
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `yarn vitest run src/components/environments/EnvironmentDialog.test.tsx`
Expected: the new `describe` FAILS, for example `Unable to find an accessible element with the role "tab" and name /certificates/i`. The earlier tests in the file still PASS with the plain-input editor mock.

- [ ] **Step 3: Update the imports**

In `src/components/environments/EnvironmentDialog.tsx` replace

```typescript
import { useSaveButton } from '@/hooks/use-save-button';
import { validateExternalSecretBindings } from '@/lib/external-secrets';
```

with

```typescript
import { useSaveButton } from '@/hooks/use-save-button';
import { validateClientCertificates } from '@/lib/certificate-validation';
import { validateExternalSecretBindings } from '@/lib/external-secrets';
```

Replace

```typescript
import type { Environment, ExternalSecretBinding, Variable } from '@/lib/tauri-api';
```

with

```typescript
import type {
  ClientCertificate,
  Environment,
  ExternalSecretBinding,
  Variable,
} from '@/lib/tauri-api';
```

Replace

```typescript
import { EnvironmentSidebar } from './EnvironmentSidebar';
```

with

```typescript
import { CertificatesTab } from './CertificatesTab';
import { EnvironmentSidebar } from './EnvironmentSidebar';
```

- [ ] **Step 4: Widen the normalized type and the tab type**

Replace

```typescript
type NormalizedEnvironment = Environment & { externalSecrets: ExternalSecretBinding[] };

function normalizeEnv(env: Environment): NormalizedEnvironment {
  return { ...env, externalSecrets: env.externalSecrets ?? [] };
}
```

with

```typescript
type NormalizedEnvironment = Environment & {
  externalSecrets: ExternalSecretBinding[];
  clientCertificates: ClientCertificate[];
};

function normalizeEnv(env: Environment): NormalizedEnvironment {
  return {
    ...env,
    externalSecrets: env.externalSecrets ?? [],
    clientCertificates: env.clientCertificates ?? [],
  };
}

type DialogTab = 'variables' | 'external-secrets' | 'certificates';
```

Replace

```typescript
  const [activeDialogTab, setActiveDialogTab] = useState<'variables' | 'external-secrets'>(
    'variables',
  );
```

with

```typescript
  const [activeDialogTab, setActiveDialogTab] = useState<DialogTab>('variables');
```

The tab-reset effect (`setActiveDialogTab('variables')` keyed on `selectedName`) needs no change: selecting another environment already returns to Variables.

- [ ] **Step 5: Validate certificates in `handleSave`**

Replace

```typescript
  // Both tabs share one save, so an unfinished binding blocks a Variables save too.
  // Point the user at the External Secrets tab instead of failing with a generic toast.
  const handleSave = useCallback(() => {
    if (!selectedEnv) return;
    const error = validateExternalSecretBindings(selectedEnv.externalSecrets);
    if (error) {
      toast.error(error);
      setActiveDialogTab('external-secrets');
      return;
    }
    void triggerSave();
  }, [selectedEnv, triggerSave]);
```

with

```typescript
  // All tabs share one save, so an unfinished binding or certificate blocks a Variables save too.
  // Point the user at the tab that holds the problem instead of failing with a generic toast.
  const handleSave = useCallback(() => {
    if (!selectedEnv) return;
    const error = validateExternalSecretBindings(selectedEnv.externalSecrets);
    if (error) {
      toast.error(error);
      setActiveDialogTab('external-secrets');
      return;
    }
    // Certificate references are checked against the bindings above, which are now known valid.
    const issues = validateClientCertificates(
      selectedEnv.clientCertificates,
      selectedEnv.externalSecrets,
    );
    if (issues.errors.length > 0) {
      toast.error(issues.errors[0]);
      setActiveDialogTab('certificates');
      return;
    }
    // Warnings (a literal passphrase) never block the save.
    if (issues.warnings.length > 0) toast.warning(issues.warnings[0]);
    void triggerSave();
  }, [selectedEnv, triggerSave]);
```

- [ ] **Step 6: Add the certificate handlers**

Insert after the `removeExternalSecret` callback (which ends at line 287, just before `const { data: globalEnvName ...`):

```typescript
  const updateClientCertificate = useCallback(
    (idx: number, patch: Partial<ClientCertificate>) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          const clientCertificates = e.clientCertificates.slice();
          clientCertificates[idx] = { ...clientCertificates[idx], ...patch } as ClientCertificate;
          return { ...e, clientCertificates };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const addClientCertificate = useCallback(
    (type: 'pem' | 'pkcs12') => {
      if (!selectedEnv) return;
      const fresh: ClientCertificate =
        type === 'pem'
          ? { type: 'pem', domain: '', certificateFilePath: '', privateKeyFilePath: '' }
          : { type: 'pkcs12', domain: '', pkcs12FilePath: '' };
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          return { ...e, clientCertificates: [...e.clientCertificates, fresh] };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const removeClientCertificate = useCallback(
    (idx: number) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          return { ...e, clientCertificates: e.clientCertificates.filter((_, i) => i !== idx) };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  // The first matching certificate wins, so the order is part of the data.
  const moveClientCertificate = useCallback(
    (idx: number, direction: -1 | 1) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          const target = idx + direction;
          if (target < 0 || target >= e.clientCertificates.length) return e;
          const clientCertificates = e.clientCertificates.slice();
          [clientCertificates[idx], clientCertificates[target]] = [
            clientCertificates[target],
            clientCertificates[idx],
          ];
          return { ...e, clientCertificates };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );
```

- [ ] **Step 7: Render the third tab**

Replace

```tsx
                onValueChange={(v) => setActiveDialogTab(v as 'variables' | 'external-secrets')}
```

with

```tsx
                onValueChange={(v) => setActiveDialogTab(v as DialogTab)}
```

Replace

```tsx
                  <TabsTrigger value='external-secrets' className='text-xs'>
                    External Secrets
                  </TabsTrigger>
                </TabsList>
```

with

```tsx
                  <TabsTrigger value='external-secrets' className='text-xs'>
                    External Secrets
                  </TabsTrigger>
                  <TabsTrigger value='certificates' className='text-xs'>
                    Certificates
                  </TabsTrigger>
                </TabsList>
```

Replace

```tsx
                    saveState={saveState}
                  />
                </TabsContent>
              </Tabs>
```

with

```tsx
                    saveState={saveState}
                  />
                </TabsContent>
                <TabsContent value='certificates' className='flex-1 flex flex-col min-h-0 m-0'>
                  <CertificatesTab
                    certificates={selectedEnv.clientCertificates}
                    bindings={selectedEnv.externalSecrets}
                    onChange={updateClientCertificate}
                    onAdd={addClientCertificate}
                    onRemove={removeClientCertificate}
                    onMove={moveClientCertificate}
                    onSave={handleSave}
                    isDirty={isDirty}
                    saveState={saveState}
                    variableContext={variableContext}
                  />
                </TabsContent>
              </Tabs>
```

(The `saveState={saveState} />` plus `</TabsContent>` plus `</Tabs>` sequence is unique: it follows the `ExternalSecretsTab` block.)

- [ ] **Step 8: Run the tests and confirm they pass**

Run: `yarn vitest run src/components/environments/`
Expected: PASS for `EnvironmentDialog.test.tsx`, `CertificatesTab.test.tsx` and `ExternalSecretsTab.test.tsx`.

- [ ] **Step 9: Run the checks**

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS. If Biome reports formatting only, run `yarn format` and re-run `yarn check`.

Run: `yarn vitest run src/lib src/components/environments src/components/workspace`
Expected: PASS (the workspace environments tab shares the widened `Environment` type).

- [ ] **Step 10: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage only these paths, then commit with the same paths:

```bash
git add src/components/environments/EnvironmentDialog.tsx src/components/environments/EnvironmentDialog.test.tsx
git commit -- src/components/environments/EnvironmentDialog.tsx src/components/environments/EnvironmentDialog.test.tsx
```

Suggested message: `feat(environments): add a Certificates tab to the environment dialog`, with a body that says save validation mirrors the backend rules and warnings do not block, ending with `Relates to: #21`.

---

## Global environment editor: intentionally not changed

`src/components/workspace/WorkspaceEnvironmentsTab.tsx` (the workspace-wide global environment editor) is intentionally not changed. It edits variables only. It saves with `{ ...env, variables: editingVars }` (line 36), so a global environment that already carries `clientCertificates` keeps them on save, and it needs no new tab. Certificates are managed per collection environment in `EnvironmentDialog`.

## Manual check (real app)

Run `yarn tauri dev` with a collection open. Plan B2 must be merged so the backend accepts the new fields. Plan C makes requests resolve vault references, so steps 9 and 10 need it, and a RocketVault connection.

1. Open **Manage Environments** and select (or create) an environment, for example `prod`. See three tabs: Variables, External Secrets, Certificates.
2. Open **Certificates**. See the empty state "No client certificates", the two hint lines (wildcards and ports, relative paths and encrypted keys) and the buttons **Add PEM** and **Add PKCS12**. Save is disabled.
3. Click **Add PEM**. See a row with a `PEM` badge, an empty domain field, a Certificate and a Private key piece (each with a `File` source selector, a path field and **Browse**) and a masked Passphrase field. Save becomes enabled.
4. Type the domain `api.example.com`. Click **Browse** for the certificate and pick a `.pem` file in the native file dialog. See the path appear. Type a relative path such as `certs/client.key` for the key.
5. Click **Add PKCS12** with domain `*.example.com` and click Save. See a toast that the PKCS12 bundle needs a file path or a vault secret, and the dialog stays open on the Certificates tab. Fill in a path and Save again. See the button show Saved.
6. Move the `*.example.com` row up with the arrow button and Save. Close and reopen the dialog. See the order kept. Open the environment YAML under `<collection>/environments/` and see `clientCertificates` in that order, with no key text.
7. Type a literal passphrase `hunter2` in a row. See the amber text "saved in the environment file as typed". Click Save. See a warning toast, and that the save still succeeds.
8. Try `../secret/client.p12` as a PKCS12 path and Save. See the error "must not contain `..`" and no save. Paste text starting with `-----BEGIN PRIVATE KEY-----` into a path field and Save. See the error that names the field and says it is not key text.
9. On **External Secrets** add a binding with alias `vault`, pick a connection and vault, and click **Fetch Secrets**. Back on Certificates switch the Certificate source to `Vault secret`. See the path field replaced by a picker that lists `vault.<secretName>` entries. Pick one. Open the passphrase "Insert vault secret" picker, pick a secret, and see `{{vault.<name>}}` in the passphrase field and no warning. Save.
10. Delete the secret from the vault side or change the binding's vault, fetch again and Save with the stale reference. See the error that the reference is not one of the fetched secrets, and the dialog return to Certificates. Switch a row from Vault secret back to File and see the secret cleared.
11. Send a request to a host that matches a saved certificate and see it use the certificate (Plans B and C).

## Next Plan

This is the last plan in the series (B, C, D). Plan E, a native RocketVault certificate source, is not in this index. It follows RocketVault's published certificate export spec and its implementation, and is specced separately (section 10 of the design spec). The vault references written by this UI (`alias.secretName`) are the seam Plan E builds on.
