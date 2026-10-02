# RocketVault Certificate Source, Plan D: Certificates UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user add a RocketVault certificate on the Certificates tab: "Add RocketVault certificate" creates an entry with a binding dropdown (the environment's External Secrets aliases), a certificate dropdown loaded from RocketVault, and a format dropdown (PEM by default). Non-exportable certificates are listed but disabled, each name shows its key algorithm, a stored name missing from the vault shows "not found", and an EC certificate picked as PEM on Windows shows a warning that suggests PKCS12. Save runs the same rules as the backend.

**Architecture:** D1 widens the `ClientCertificate` type with the `vault` variant, mirrors A2's save rules in `certificate-validation.ts`, and adds `src/lib/vault-certificates.ts` (key-algorithm and Windows rules, picker labels) with the OS read through `@tauri-apps/plugin-os`, which the app already uses. D2 adds `VaultCertificateRow.tsx`, the body of a vault row, which loads the list through C3's `listVaultCertificates` and drops answers for an old binding. D3 renders it from `CertificatesTab`, adds the third Add button, and lets `EnvironmentDialog` create an empty vault entry. No new store state: props and the dialog's existing local state only.

**Tech Stack:** React, TypeScript, shadcn/ui (`Button`, `Badge`, `Select`), lucide-react, `@tauri-apps/plugin-os`, Vitest, @testing-library/react, @testing-library/user-event, Biome.

**Spec:** [`docs/superpowers/specs/2026-10-02-vault-certificate-source-design.md`](../../specs/2026-10-02-vault-certificate-source-design.md) (sections 2.2, 4, 7, 8, 10, 11.2). Plan index and the shared interface contract: [`00-plan-index.md`](00-plan-index.md).

**Plan D of 4 (A, B, C, D).** Depends on A2 (the persisted shape the backend accepts) and C3 (`listVaultCertificates`).

---

## Global Constraints

- Values fetched from RocketVault are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references persist (an External Secrets alias and a certificate name). Runtime material is `zeroize::Zeroizing` from fetch to use.
- An unresolved or failed vault certificate fails a request or token request only when that certificate is the one selected for the URL, with no fallback to another entry. An entry for another domain causes no RocketVault call.
- A `CertificateMaterial::Deferred` that reaches the executor is an `InvalidInput` error, never a silent skip.
- A path or reference field must not hold key text: a value starting with `-----BEGIN` is rejected on save. For a `vault` entry this covers every field.
- Error and log text never contains key bytes, PKCS12 bundle bytes or the one-time password.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, plus the `ClientCertificate` variants, which already carry it. Persisted fields stay backward compatible (additive, with defaults).
- Rust: never call `unwrap` in production paths. Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Frontend: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`), `lucide-react` icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco only for multi-line editors, never fully destructure Zustand store state at component top level. Checks: `yarn tsc --noEmit`, `yarn check`, `yarn vitest run <path>`.
- Commits: conventional commits, path-scoped (`git add <paths>` then `git commit -- <paths>`, never `git add -A` or `git commit -a`). Every commit goes through the `dev-workflow-skills:1-git-commit` skill. Commit messages end with `Relates to: #21`.
- Every task that touches collection, environment or certificate data models starts with: `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`

## Review Focus (items this plan owns)

2. **A stored name that is no longer in the vault.** Owner D2 (picker half). Pinned by `shows a stored name that is no longer in the vault as not found` in `src/components/environments/VaultCertificateRow.test.tsx`.
5. **A certificate that cannot be exported as configured.** Owner D2 (picker half). Pinned by `lists a non-exportable certificate as disabled with its key algorithm` and `warns about an EC certificate with PEM on Windows only` in `VaultCertificateRow.test.tsx`.

Also pinned here: the frontend mirror of A2's rules (`rejects a binding that is not in the environment`, `rejects key text in any vault field without echoing it` in `src/lib/certificate-validation.test.ts`, D1) and the dialog wiring (`blocks the save when a RocketVault certificate names a binding the environment lacks` in `EnvironmentDialog.test.tsx`, D3).

## Frontend rules for this plan

- shadcn/ui primitives only: `Button`, `Badge`, `Select`, `ScrollArea`. No raw `<button>`, `<input>`, `<select>`, `<form>`, `<dialog>`.
- Icons from `lucide-react` only: `Plus`, `RefreshCw`, `Loader2`, `AlertTriangle`.
- The domain stays a `SingleLineEditor` (it is `{{var}}` aware). Binding, certificate and format are picks from known lists, so they are `Select`s, like the vault-secret picker in the same tab.
- Zustand: no store reads. All state is props, the dialog's local state, and the row's own list state.
- Tests replace `@/components/editor` with a plain input, mock `@/lib/tauri-api`'s `listVaultCertificates` and `@tauri-apps/plugin-os`'s `type`, and add the Radix `hasPointerCapture` and `scrollIntoView` polyfills, as `CertificatesTab.test.tsx` already does.

## Spec versus code (read before starting)

1. **OS detection** (spec risk 2). `@tauri-apps/plugin-os` is already a dependency (`package.json` line 62, `tauri-plugin-os` in `src-tauri/Cargo.toml`) and `src/App.tsx` and `src/components/title-bar/TitleBar.tsx` call its synchronous `type()`. D1 uses it, wrapped in `isWindows()` that returns false outside Tauri.
2. **The command name.** The spec writes `list_vault_certificates(connection_id, vault_name)`. Tauri maps the Rust arguments to camelCase, so the binding from C3 is `listVaultCertificates(connectionId, vaultName)`; the row takes both from the selected binding.
3. **Format is optional on disk.** The backend defaults a missing `format` to `pem` (A2), so the TypeScript type has `format?: VaultCertificateFormat` and the row shows PEM when it is absent. A new entry is created with `format: 'pem'` written out.
4. **Rows are typed by `type`.** The existing tab treats any non-PEM entry as PKCS12 (`pieceSpecs`, the badge). D1 narrows those spots to file and vault-secret entries so `yarn tsc` stays green before D3 renders the vault body.

## Findings from reading the real code (do not re-derive)

- `src/lib/tauri-api.ts`: `ExternalSecretBinding { alias, connectionId, vaultName, secretNames }` at lines 189-194, the `ClientCertificate` union right after `SecretManagerConnection` (lines 205-224 at `0c5a83e9`; C3 inserts `VaultCertificateSummary` above it, so find it by name), `listVaultCertificates` and `VaultCertificateSummary` from C3.
- `src/lib/certificate-validation.ts` (132 lines): `isKeyText` line 48, `piecesOf(cert)` lines 24-46, the main loop lines 88-129 with `who` at line 90 and the passphrase warning at lines 124-128. Tests in `src/lib/certificate-validation.test.ts` use a `bindings` fixture whose alias is `vault`.
- `src/components/environments/CertificatesTab.tsx` (453 lines): props lines 25-36, `pieceSpecs` lines 52-86, the footer Add buttons lines 143-162, the empty-state text line 118, `CertificateRow` lines 183-275 (badge line 210, pieces and passphrase lines 253-272).
- `src/components/environments/EnvironmentDialog.tsx`: `addClientCertificate` lines 331-346 (`type: 'pem' | 'pkcs12'`); `CertificatesTab` is rendered at lines 471-482 with `onAdd={addClientCertificate}`. `handleSave` already runs `validateClientCertificates` and toasts the first error.
- `src/components/environments/EnvironmentDialog.test.tsx` mocks `@/lib/tauri-api` with `importActual` plus overrides at lines 13-24, mocks `sonner`, and has a `describe('EnvironmentDialog certificates tab')` block with `vaultBinding` (alias `vault`) at lines 186-329.

---

## Task D1: Types, validation mirror and the Windows EC rule

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Modify: `src/lib/tauri-api.ts` (the `ClientCertificate` union and its comment)
- Modify: `src/lib/certificate-validation.ts`
- Modify: `src/lib/certificate-validation.test.ts` (new `describe` at the end)
- Create: `src/lib/vault-certificates.ts`
- Create: `src/lib/vault-certificates.test.ts`
- Modify: `src/components/environments/CertificatesTab.tsx` (`pieceSpecs` signature line 52, badge line 210, body lines 253-272; type narrowing only)

**Interfaces:**
- Consumes: C3's `VaultCertificateSummary`.
- Produces (contract names): `VaultCertificateFormat`, the `vault` member of `ClientCertificate`, `validateClientCertificates` with vault rules, `isEcKeyAlgorithm`, `isWindows`, `needsEcPemWarning`, `isSelectable`, `vaultCertificateLabel`. Consumed by D2 and D3.

- [ ] **Step 1: Write the failing validation tests**

Append to `src/lib/certificate-validation.test.ts`:

```typescript
describe('validateClientCertificates for RocketVault certificates', () => {
  type VaultCert = Extract<ClientCertificate, { type: 'vault' }>;
  const KEY_TEXT = '-----BEGIN PRIVATE KEY-----\nMIIEvQsecret\n-----END PRIVATE KEY-----';

  function vaultCert(overrides: Partial<VaultCert> = {}): ClientCertificate {
    return {
      type: 'vault',
      domain: 'api.example.com',
      binding: 'vault',
      certificate: 'client-a',
      format: 'pem',
      ...overrides,
    };
  }

  it('accepts a vault entry with a bound alias and a certificate name', () => {
    expect(validateClientCertificates([vaultCert()], bindings)).toEqual({ errors: [], warnings: [] });
  });

  it('accepts a vault entry with no format, which means PEM', () => {
    const cert: ClientCertificate = {
      type: 'vault',
      domain: 'api.example.com',
      binding: 'vault',
      certificate: 'client-a',
    };
    expect(validateClientCertificates([cert], bindings).errors).toEqual([]);
  });

  it('rejects a binding that is not in the environment', () => {
    const { errors } = validateClientCertificates([vaultCert({ binding: 'payments' })], bindings);
    expect(errors).toEqual([
      'Certificate 1 (api.example.com): binding "payments" has no External Secrets binding in this environment.',
    ]);
  });

  it('asks for a binding and a certificate', () => {
    const { errors } = validateClientCertificates(
      [vaultCert({ binding: ' ', certificate: '' })],
      bindings,
    );
    expect(errors).toEqual([
      'Certificate 1 (api.example.com): choose an External Secrets binding.',
      'Certificate 1 (api.example.com): choose a certificate.',
    ]);
  });

  it('rejects key text in any vault field without echoing it', () => {
    for (const [field, cert] of [
      ['domain', vaultCert({ domain: KEY_TEXT })],
      ['binding', vaultCert({ binding: KEY_TEXT })],
      ['certificate', vaultCert({ certificate: KEY_TEXT })],
    ] as const) {
      const { errors } = validateClientCertificates([cert], bindings);
      expect(errors).toHaveLength(1);
      expect(errors[0]).toContain(`${field} must be a name, not key text`);
      expect(errors[0]).not.toContain('MIIEvQsecret');
    }
  });

  it('rejects an unknown format', () => {
    const cert = vaultCert({ format: 'der' as unknown as VaultCert['format'] });
    expect(validateClientCertificates([cert], bindings).errors).toEqual([
      'Certificate 1 (api.example.com): format must be PEM or PKCS12.',
    ]);
  });

  it('never warns about a passphrase for a vault entry', () => {
    expect(validateClientCertificates([vaultCert()], bindings).warnings).toEqual([]);
  });
});
```

- [ ] **Step 2: Write the failing helper tests**

Create `src/lib/vault-certificates.test.ts`:

```typescript
// src/lib/vault-certificates.test.ts

import { type as osType } from '@tauri-apps/plugin-os';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VaultCertificateSummary } from '@/lib/tauri-api';
import {
  isEcKeyAlgorithm,
  isSelectable,
  isWindows,
  needsEcPemWarning,
  vaultCertificateLabel,
} from '@/lib/vault-certificates';

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));

function summary(overrides: Partial<VaultCertificateSummary> = {}): VaultCertificateSummary {
  return {
    id: '1',
    name: 'client-a',
    exportable: true,
    enabled: true,
    keyAlgorithm: 'RSA-2048',
    expiresAt: null,
    ...overrides,
  };
}

describe('isEcKeyAlgorithm', () => {
  it('is true for the EC spellings RocketVault may use', () => {
    for (const alg of ['EC', 'ECDSA', 'EC-P256', ' ec-p384 ']) {
      expect(isEcKeyAlgorithm(alg)).toBe(true);
    }
  });

  it('is false for RSA, Ed25519 and a missing value', () => {
    for (const alg of ['RSA-2048', 'Ed25519', '', undefined]) {
      expect(isEcKeyAlgorithm(alg)).toBe(false);
    }
  });
});

describe('isWindows', () => {
  beforeEach(() => {
    vi.mocked(osType).mockReset();
  });

  it('follows the Tauri OS type', () => {
    vi.mocked(osType).mockReturnValue('windows');
    expect(isWindows()).toBe(true);
    vi.mocked(osType).mockReturnValue('linux');
    expect(isWindows()).toBe(false);
  });

  it('is false outside Tauri', () => {
    vi.mocked(osType).mockImplementation(() => {
      throw new Error('not running in Tauri');
    });
    expect(isWindows()).toBe(false);
  });
});

describe('needsEcPemWarning', () => {
  it('warns only for an EC key exported as PEM on Windows', () => {
    expect(needsEcPemWarning('EC-P256', 'pem', true)).toBe(true);
    expect(needsEcPemWarning('EC-P256', 'pkcs12', true)).toBe(false);
    expect(needsEcPemWarning('EC-P256', 'pem', false)).toBe(false);
    expect(needsEcPemWarning('RSA-2048', 'pem', true)).toBe(false);
    expect(needsEcPemWarning(undefined, 'pem', true)).toBe(false);
  });
});

describe('vaultCertificateLabel and isSelectable', () => {
  it('shows the name and key algorithm, and why a certificate cannot be picked', () => {
    expect(vaultCertificateLabel(summary())).toBe('client-a · RSA-2048');
    expect(vaultCertificateLabel(summary({ name: 'locked', exportable: false }))).toBe(
      'locked · RSA-2048 · not exportable',
    );
    expect(vaultCertificateLabel(summary({ name: 'old', enabled: false }))).toBe(
      'old · RSA-2048 · disabled',
    );
    expect(vaultCertificateLabel(summary({ name: 'bare', keyAlgorithm: '' }))).toBe('bare');
  });

  it('allows only exportable, enabled certificates', () => {
    expect(isSelectable(summary())).toBe(true);
    expect(isSelectable(summary({ exportable: false }))).toBe(false);
    expect(isSelectable(summary({ enabled: false }))).toBe(false);
  });
});
```

- [ ] **Step 3: Run them to verify they fail**

Run: `yarn vitest run src/lib/certificate-validation.test.ts src/lib/vault-certificates.test.ts`
Expected: FAIL: `vault-certificates.test.ts` cannot resolve `@/lib/vault-certificates`, and the vault tests in `certificate-validation.test.ts` fail (the vault entry is treated as PKCS12 and reports "PKCS12 bundle needs a file path or a vault secret").

- [ ] **Step 4: Widen the type**

In `src/lib/tauri-api.ts`, the `ClientCertificate` comment and union, old:

```typescript
// Persisted client certificate. A piece has one source: a file path or a
// vault reference (`alias.secretName`). A reference is never a value.
export type ClientCertificate =
```

new:

```typescript
// How a RocketVault certificate is exported.
export type VaultCertificateFormat = 'pem' | 'pkcs12';

// Persisted client certificate. For PEM and PKCS12, a piece has one source: a
// file path or a vault reference (`alias.secretName`). A reference is never a
// value. A `vault` entry names a RocketVault certificate that is exported when
// a request needs it; it stores names only. A missing format means PEM.
export type ClientCertificate =
```

and the end of the union, old:

```typescript
  | {
      type: 'pkcs12';
      domain: string;
      pkcs12FilePath?: string;
      pkcs12Secret?: string;
      passphrase?: string;
    };
```

new:

```typescript
  | {
      type: 'pkcs12';
      domain: string;
      pkcs12FilePath?: string;
      pkcs12Secret?: string;
      passphrase?: string;
    }
  | {
      type: 'vault';
      domain: string;
      // External Secrets alias of this environment; the connection and vault come from it.
      binding: string;
      // Certificate name in that vault.
      certificate: string;
      format?: VaultCertificateFormat;
    };
```

- [ ] **Step 5: Add the helpers**

Create `src/lib/vault-certificates.ts`:

```typescript
// src/lib/vault-certificates.ts

import { type as osType } from '@tauri-apps/plugin-os';
import type { VaultCertificateFormat, VaultCertificateSummary } from '@/lib/tauri-api';

// True for an elliptic-curve key, in any spelling RocketVault may use (EC, ECDSA, EC-P256).
export function isEcKeyAlgorithm(keyAlgorithm: string | undefined): boolean {
  return !!keyAlgorithm && /^ec/i.test(keyAlgorithm.trim());
}

// True when the app runs on Windows. Outside Tauri (tests, a browser) it is false.
export function isWindows(): boolean {
  try {
    return osType() === 'windows';
  } catch {
    return false;
  }
}

// The Windows TLS stack may not load an EC key from PEM, so the picker suggests PKCS12 there.
export function needsEcPemWarning(
  keyAlgorithm: string | undefined,
  format: VaultCertificateFormat,
  windows: boolean,
): boolean {
  return windows && format === 'pem' && isEcKeyAlgorithm(keyAlgorithm);
}

// A certificate can be picked only when RocketVault can export it.
export function isSelectable(summary: VaultCertificateSummary): boolean {
  return summary.exportable && summary.enabled;
}

// The picker label: the name, the key algorithm, and why it cannot be picked.
export function vaultCertificateLabel(summary: VaultCertificateSummary): string {
  let reason = '';
  if (!summary.exportable) reason = 'not exportable';
  else if (!summary.enabled) reason = 'disabled';
  return [summary.name, summary.keyAlgorithm, reason].filter(Boolean).join(' · ');
}
```

- [ ] **Step 6: Mirror the save rules**

In `src/lib/certificate-validation.ts`, the import line 3, old:

```typescript
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';
```

new:

```typescript
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';

type FileCertificate = Exclude<ClientCertificate, { type: 'vault' }>;
type VaultCertificate = Extract<ClientCertificate, { type: 'vault' }>;

const VAULT_FORMATS: readonly string[] = ['pem', 'pkcs12'];
```

`piecesOf` signature (line 24), old:

```typescript
function piecesOf(cert: ClientCertificate): Piece[] {
```

new:

```typescript
function piecesOf(cert: FileCertificate): Piece[] {
```

After `isKeyText` (lines 48-50), add:

```typescript

// Mirrors the `vault` rules of `validate_client_certificates` in rocket-environment.
function vaultCertificateErrors(
  who: string,
  cert: VaultCertificate,
  bindings: ExternalSecretBinding[],
): string[] {
  const named: [string, string][] = [
    ['domain', cert.domain],
    ['binding', cert.binding],
    ['certificate', cert.certificate],
  ];
  const keyText = named
    .filter(([, value]) => isKeyText(value))
    .map(([field]) => `${who}: ${field} must be a name, not key text.`);
  if (keyText.length > 0) return keyText;

  const errors: string[] = [];
  if (cert.binding.trim() === '') {
    errors.push(`${who}: choose an External Secrets binding.`);
  } else if (!bindings.some((b) => b.alias === cert.binding)) {
    errors.push(
      `${who}: binding "${cert.binding}" has no External Secrets binding in this environment.`,
    );
  }
  if (cert.certificate.trim() === '') errors.push(`${who}: choose a certificate.`);
  if (cert.format !== undefined && !VAULT_FORMATS.includes(cert.format)) {
    errors.push(`${who}: format must be PEM or PKCS12.`);
  }
  return errors;
}
```

In the main loop, old:

```typescript
    const domain = cert.domain.trim();
    const who = domain ? `Certificate ${idx + 1} (${domain})` : `Certificate ${idx + 1}`;
    if (!domain) errors.push(`${who}: domain is required.`);

    for (const piece of piecesOf(cert)) {
```

new:

```typescript
    const domain = cert.domain.trim();
    // A pasted key in the domain must not be echoed back in every message.
    const who =
      domain && !isKeyText(domain) ? `Certificate ${idx + 1} (${domain})` : `Certificate ${idx + 1}`;
    if (!domain) errors.push(`${who}: domain is required.`);

    if (cert.type === 'vault') {
      errors.push(...vaultCertificateErrors(who, cert, bindings));
      continue;
    }

    for (const piece of piecesOf(cert)) {
```

- [ ] **Step 7: Keep the Certificates tab compiling**

In `src/components/environments/CertificatesTab.tsx`, after the imports (line 23), add:

```typescript

type FileCertificate = Exclude<ClientCertificate, { type: 'vault' }>;

const TYPE_LABELS: Record<ClientCertificate['type'], string> = {
  pem: 'PEM',
  pkcs12: 'PKCS12',
  vault: 'Vault',
};
```

`pieceSpecs` (line 52), old:

```typescript
function pieceSpecs(cert: ClientCertificate): PieceSpec[] {
```

new:

```typescript
function pieceSpecs(cert: FileCertificate): PieceSpec[] {
```

The badge in `CertificateRow` (line 210), old:

```tsx
          {cert.type === 'pem' ? 'PEM' : 'PKCS12'}
```

new:

```tsx
          {TYPE_LABELS[cert.type]}
```

The pieces and passphrase in `CertificateRow` (lines 253-272), old:

```tsx
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
```

new (Task D3 adds the vault body):

```tsx
      {cert.type !== 'vault' && (
        <>
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
        </>
      )}
    </div>
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `yarn vitest run src/lib/certificate-validation.test.ts src/lib/vault-certificates.test.ts src/components/environments/CertificatesTab.test.tsx`
Expected: PASS, all tests in the three files (the existing `CertificatesTab` tests are unchanged).

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no Biome errors in the touched files. If it reports formatting only, run `yarn format` and re-run the tests; never reformat files this task does not list.

- [ ] **Step 9: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add src/lib/tauri-api.ts src/lib/certificate-validation.ts src/lib/certificate-validation.test.ts src/lib/vault-certificates.ts src/lib/vault-certificates.test.ts src/components/environments/CertificatesTab.tsx
git commit -- src/lib/tauri-api.ts src/lib/certificate-validation.ts src/lib/certificate-validation.test.ts src/lib/vault-certificates.ts src/lib/vault-certificates.test.ts src/components/environments/CertificatesTab.tsx
```

Suggested subject: `feat(ui): add the vault certificate type and its save rules`. The message ends with `Relates to: #21`.

---

## Task D2: `VaultCertificateRow` picker

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Create: `src/components/environments/VaultCertificateRow.tsx`
- Create: `src/components/environments/VaultCertificateRow.test.tsx`

**Interfaces:**
- Consumes: C3's `listVaultCertificates` and `VaultCertificateSummary`; D1's `VaultCertificateFormat`, `isSelectable`, `isWindows`, `needsEcPemWarning`, `vaultCertificateLabel`.
- Produces (contract names): `VaultCertificateRow`, `VaultCertificateRowProps { idx, cert, bindings, onChange }`. Consumed by D3.

- [ ] **Step 1: Write the failing row tests**

Create `src/components/environments/VaultCertificateRow.test.tsx`:

```tsx
// src/components/environments/VaultCertificateRow.test.tsx

import { type as osType } from '@tauri-apps/plugin-os';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { VaultCertificateRow } from '@/components/environments/VaultCertificateRow';
import type {
  ClientCertificate,
  ExternalSecretBinding,
  VaultCertificateSummary,
} from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listVaultCertificates: vi.fn() };
});

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op for test polyfill.
  };
}

type VaultCert = Extract<ClientCertificate, { type: 'vault' }>;

const bindings: ExternalSecretBinding[] = [
  { alias: 'prod', connectionId: 'conn-1', vaultName: 'prod-vault', secretNames: [] },
  { alias: 'staging', connectionId: 'conn-2', vaultName: 'stage-vault', secretNames: [] },
];

const listed: VaultCertificateSummary[] = [
  { id: '1', name: 'client-a', exportable: true, enabled: true, keyAlgorithm: 'RSA-2048', expiresAt: null },
  { id: '2', name: 'locked', exportable: false, enabled: true, keyAlgorithm: 'RSA-4096', expiresAt: null },
  { id: '3', name: 'edge', exportable: true, enabled: true, keyAlgorithm: 'EC-P256', expiresAt: null },
];

function renderRow(overrides: Partial<VaultCert> = {}) {
  const onChange = vi.fn();
  const cert: VaultCert = {
    type: 'vault',
    domain: 'api.example.com',
    binding: 'prod',
    certificate: 'client-a',
    format: 'pem',
    ...overrides,
  };
  const view = render(
    <VaultCertificateRow idx={0} cert={cert} bindings={bindings} onChange={onChange} />,
  );
  return { onChange, ...view };
}

const certificatePicker = () =>
  screen.getByRole('combobox', { name: 'Vault certificate for certificate 1' });

describe('VaultCertificateRow', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listVaultCertificates).mockReset().mockResolvedValue(listed);
    vi.mocked(osType).mockReturnValue('linux');
  });

  it('loads the certificates of the chosen binding', async () => {
    renderRow();
    await waitFor(() =>
      expect(tauriApi.listVaultCertificates).toHaveBeenCalledWith('conn-1', 'prod-vault'),
    );
    await waitFor(() => expect(certificatePicker()).toHaveTextContent('client-a · RSA-2048'));
  });

  // Review Focus 5.
  it('lists a non-exportable certificate as disabled with its key algorithm', async () => {
    renderRow({ certificate: '' });
    const user = userEvent.setup();
    await waitFor(() => expect(tauriApi.listVaultCertificates).toHaveBeenCalled());
    await user.click(certificatePicker());
    const locked = await screen.findByRole('option', { name: 'locked · RSA-4096 · not exportable' });
    expect(locked).toHaveAttribute('aria-disabled', 'true');
    expect(screen.getByRole('option', { name: 'client-a · RSA-2048' })).not.toHaveAttribute(
      'aria-disabled',
      'true',
    );
  });

  // Review Focus 2.
  it('shows a stored name that is no longer in the vault as not found', async () => {
    renderRow({ certificate: 'gone' });
    expect(
      await screen.findByText('Certificate gone was not found in this vault.'),
    ).toBeInTheDocument();
    expect(certificatePicker()).toHaveTextContent('gone (not found)');
  });

  // Review Focus 5.
  it('warns about an EC certificate with PEM on Windows only', async () => {
    vi.mocked(osType).mockReturnValue('windows');
    const pemOnWindows = renderRow({ certificate: 'edge', format: 'pem' });
    expect(await screen.findByText(/has an EC key/)).toBeInTheDocument();
    pemOnWindows.unmount();

    const pkcs12OnWindows = renderRow({ certificate: 'edge', format: 'pkcs12' });
    await waitFor(() => expect(certificatePicker()).toHaveTextContent('edge · EC-P256'));
    expect(screen.queryByText(/has an EC key/)).not.toBeInTheDocument();
    pkcs12OnWindows.unmount();

    vi.mocked(osType).mockReturnValue('linux');
    renderRow({ certificate: 'edge', format: 'pem' });
    await waitFor(() => expect(certificatePicker()).toHaveTextContent('edge · EC-P256'));
    expect(screen.queryByText(/has an EC key/)).not.toBeInTheDocument();
  });

  it('changing the binding clears the certificate', async () => {
    const { onChange } = renderRow();
    const user = userEvent.setup();
    await user.click(screen.getByRole('combobox', { name: 'Binding for certificate 1' }));
    await user.click(await screen.findByRole('option', { name: 'staging' }));
    expect(onChange).toHaveBeenCalledWith(0, { binding: 'staging', certificate: '' });
  });

  it('picks a certificate and a format, with PEM as the default', async () => {
    const { onChange } = renderRow({ certificate: '', format: undefined });
    const user = userEvent.setup();
    expect(screen.getByRole('combobox', { name: 'Format for certificate 1' })).toHaveTextContent(
      'PEM',
    );

    await user.click(certificatePicker());
    await user.click(await screen.findByRole('option', { name: 'client-a · RSA-2048' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificate: 'client-a' });

    await user.click(screen.getByRole('combobox', { name: 'Format for certificate 1' }));
    await user.click(await screen.findByRole('option', { name: 'PKCS12' }));
    expect(onChange).toHaveBeenCalledWith(0, { format: 'pkcs12' });
  });

  it('shows the error when the list cannot be loaded', async () => {
    vi.mocked(tauriApi.listVaultCertificates).mockRejectedValue(
      'HTTP error: RocketVault rejected the access token (401).',
    );
    renderRow();
    expect(await screen.findByText(/Could not list certificates: .*\(401\)/)).toBeInTheDocument();
  });

  it('reloads the list on demand', async () => {
    renderRow();
    const reload = screen.getByRole('button', {
      name: 'Reload vault certificates for certificate 1',
    });
    await waitFor(() => expect(reload).toBeEnabled());
    await userEvent.setup().click(reload);
    await waitFor(() => expect(tauriApi.listVaultCertificates).toHaveBeenCalledTimes(2));
  });

  it('marks a binding that is not in the environment and lists nothing', async () => {
    renderRow({ binding: 'payments' });
    expect(
      await screen.findByText('Binding payments is not in this environment. Pick one from the list.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Binding for certificate 1' })).toHaveTextContent(
      'payments (not in this environment)',
    );
    expect(tauriApi.listVaultCertificates).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn vitest run src/components/environments/VaultCertificateRow.test.tsx`
Expected: FAIL, the import `@/components/environments/VaultCertificateRow` cannot be resolved.

- [ ] **Step 3: Implement the row**

Create `src/components/environments/VaultCertificateRow.tsx`:

```tsx
// src/components/environments/VaultCertificateRow.tsx

import { AlertTriangle, Loader2, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type ClientCertificate,
  type ExternalSecretBinding,
  listVaultCertificates,
  type VaultCertificateFormat,
  type VaultCertificateSummary,
} from '@/lib/tauri-api';
import {
  isSelectable,
  isWindows,
  needsEcPemWarning,
  vaultCertificateLabel,
} from '@/lib/vault-certificates';

type VaultCertificate = Extract<ClientCertificate, { type: 'vault' }>;

export interface VaultCertificateRowProps {
  idx: number;
  cert: VaultCertificate;
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ClientCertificate>) => void;
}

type ListState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ready'; certificates: VaultCertificateSummary[] }
  | { status: 'error'; message: string };

interface PickerOption {
  value: string;
  label: string;
  disabled: boolean;
}

const SECTION_LABEL =
  'text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70';

// The body of a `vault` certificate row: which binding, which certificate in its
// vault, and which export format. The list comes from RocketVault and holds names
// and metadata only; the certificate itself is exported when a request needs it.
export function VaultCertificateRow({ idx, cert, bindings, onChange }: VaultCertificateRowProps) {
  const n = idx + 1;
  const format: VaultCertificateFormat = cert.format ?? 'pem';
  const binding = bindings.find((b) => b.alias === cert.binding);
  const connectionId = binding?.connectionId ?? '';
  const vaultName = binding?.vaultName ?? '';
  const windows = useMemo(() => isWindows(), []);
  const [list, setList] = useState<ListState>({ status: 'idle' });
  // Only the latest request may set the list, so a slow answer for a binding the
  // user already left cannot replace the current one.
  const latest = useRef(0);

  const load = useCallback(() => {
    latest.current += 1;
    const request = latest.current;
    if (!connectionId || !vaultName) {
      setList({ status: 'idle' });
      return;
    }
    setList({ status: 'loading' });
    listVaultCertificates(connectionId, vaultName)
      .then((certificates) => {
        if (latest.current === request) setList({ status: 'ready', certificates });
      })
      .catch((err: unknown) => {
        if (latest.current === request) {
          setList({ status: 'error', message: err instanceof Error ? err.message : String(err) });
        }
      });
  }, [connectionId, vaultName]);

  useEffect(() => {
    load();
    return () => {
      latest.current += 1;
    };
  }, [load]);

  const listed = list.status === 'ready' ? list.certificates : [];
  const selected = listed.find((c) => c.name === cert.certificate);
  const ecWarning = needsEcPemWarning(selected?.keyAlgorithm, format, windows);

  const bindingOptions: PickerOption[] = bindings
    .filter((b) => b.alias)
    .map((b) => ({ value: b.alias, label: b.alias, disabled: false }));
  if (cert.binding && !binding) {
    bindingOptions.push({
      value: cert.binding,
      label: `${cert.binding} (not in this environment)`,
      disabled: false,
    });
  }

  const certificateOptions: PickerOption[] = listed.map((c) => ({
    value: c.name,
    label: vaultCertificateLabel(c),
    disabled: !isSelectable(c),
  }));
  // A stored name stays visible. Once the list is loaded and the name is missing, it is marked.
  if (cert.certificate && !selected) {
    certificateOptions.push({
      value: cert.certificate,
      label: list.status === 'ready' ? `${cert.certificate} (not found)` : cert.certificate,
      disabled: false,
    });
  }

  return (
    <div className='space-y-1'>
      <p className={SECTION_LABEL}>RocketVault certificate</p>
      <div className='flex items-center gap-1.5 min-w-0'>
        <Select
          value={cert.binding}
          onValueChange={(v) => {
            if (v !== cert.binding) onChange(idx, { binding: v, certificate: '' });
          }}
        >
          <SelectTrigger
            className='h-7 w-[140px] shrink-0 text-xs font-mono'
            aria-label={`Binding for certificate ${n}`}
          >
            <SelectValue placeholder='Binding' />
          </SelectTrigger>
          <SelectContent>
            {bindingOptions.map((option) => (
              <SelectItem key={option.value} value={option.value} className='text-xs font-mono'>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select
          value={cert.certificate}
          onValueChange={(v) => onChange(idx, { certificate: v })}
          disabled={!binding}
        >
          <SelectTrigger
            className='h-7 min-w-0 flex-1 text-xs font-mono'
            aria-label={`Vault certificate for certificate ${n}`}
          >
            <SelectValue
              placeholder={list.status === 'loading' ? 'Loading certificates' : 'Select a certificate'}
            />
          </SelectTrigger>
          <SelectContent>
            {certificateOptions.map((option) => (
              <SelectItem
                key={option.value}
                value={option.value}
                disabled={option.disabled}
                className='text-xs font-mono'
              >
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0'
          onClick={load}
          disabled={!binding || list.status === 'loading'}
          aria-label={`Reload vault certificates for certificate ${n}`}
        >
          {list.status === 'loading' ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : (
            <RefreshCw className='h-3.5 w-3.5 text-muted-foreground' />
          )}
        </Button>

        <Select
          value={format}
          onValueChange={(v) => onChange(idx, { format: v as VaultCertificateFormat })}
        >
          <SelectTrigger
            className='h-7 w-[96px] shrink-0 text-xs'
            aria-label={`Format for certificate ${n}`}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='pem' className='text-xs'>
              PEM
            </SelectItem>
            <SelectItem value='pkcs12' className='text-xs'>
              PKCS12
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      {cert.binding && !binding && (
        <p className='text-[11px] text-destructive'>
          Binding {cert.binding} is not in this environment. Pick one from the list.
        </p>
      )}
      {list.status === 'error' && (
        <p className='text-[11px] text-destructive'>Could not list certificates: {list.message}</p>
      )}
      {list.status === 'ready' && cert.certificate && !selected && (
        <p className='text-[11px] text-destructive'>
          Certificate {cert.certificate} was not found in this vault.
        </p>
      )}
      {ecWarning && (
        <p className='flex items-center gap-1 text-[11px] text-amber-600 dark:text-amber-500'>
          <AlertTriangle className='h-3 w-3 shrink-0' />
          This certificate has an EC key. Windows may not load an EC key from PEM, so choose PKCS12.
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run the row tests to verify they pass**

Run: `yarn vitest run src/components/environments/VaultCertificateRow.test.tsx`
Expected: PASS, 9 tests.

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no Biome errors in the two new files. If it reports formatting only, run `yarn format` and re-run the tests; never reformat files this task does not list.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add src/components/environments/VaultCertificateRow.tsx src/components/environments/VaultCertificateRow.test.tsx
git commit -- src/components/environments/VaultCertificateRow.tsx src/components/environments/VaultCertificateRow.test.tsx
```

Suggested subject: `feat(ui): add the RocketVault certificate picker`. The message ends with `Relates to: #21`.

---

## Task D3: Certificates tab and dialog integration

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Modify: `src/components/environments/CertificatesTab.tsx` (imports, props `onAdd`, hints, empty state, footer, `CertificateRow` props and body)
- Modify: `src/components/environments/CertificatesTab.test.tsx` (module mocks after line 35, two new tests)
- Modify: `src/components/environments/EnvironmentDialog.tsx` (`addClientCertificate` lines 331-346, a helper above the component)
- Modify: `src/components/environments/EnvironmentDialog.test.tsx` (mocks lines 13-24, three tests in the certificates `describe`)

**Interfaces:**
- Consumes: D2's `VaultCertificateRow`; D1's types.
- Produces (contract names): `CertificatesTabProps.onAdd: (type: ClientCertificate['type']) => void`.

- [ ] **Step 1: Write the failing tab tests**

In `src/components/environments/CertificatesTab.test.tsx`, after `vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));` (line 35), add:

```tsx
vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listVaultCertificates: vi.fn().mockResolvedValue([]) };
});
```

and add the import after line 8:

```tsx
import * as tauriApi from '@/lib/tauri-api';
```

Add inside `describe('CertificatesTab', ...)`, before its closing `});`:

```tsx
  it('adds a RocketVault certificate', async () => {
    const { onAdd } = renderTab([]);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Add RocketVault certificate' }));
    expect(onAdd).toHaveBeenCalledWith('vault');
  });

  it('renders a vault row with its pickers and no file or passphrase fields', async () => {
    renderTab([
      {
        type: 'vault',
        domain: 'api.example.com',
        binding: 'vault',
        certificate: 'client-a',
        format: 'pem',
      },
    ]);
    expect(screen.getByText('Vault')).toBeInTheDocument();
    expect(screen.getByLabelText('Domain for certificate 1')).toHaveValue('api.example.com');
    expect(screen.getByRole('combobox', { name: 'Binding for certificate 1' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Format for certificate 1' })).toHaveTextContent(
      'PEM',
    );
    expect(screen.queryByLabelText('Passphrase for certificate 1')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('combobox', { name: 'Certificate source for certificate 1' }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(tauriApi.listVaultCertificates).toHaveBeenCalledWith('conn-1', 'prod-vault'),
    );
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn vitest run src/components/environments/CertificatesTab.test.tsx`
Expected: FAIL: no button named "Add RocketVault certificate", and no binding picker in the vault row.

- [ ] **Step 3: Render the vault row and the third Add button**

In `src/components/environments/CertificatesTab.tsx`, after the import of `vaultSecretOptions` (line 23), add:

```tsx
import { VaultCertificateRow } from './VaultCertificateRow';
```

The prop (line 29), old:

```tsx
  onAdd: (type: 'pem' | 'pkcs12') => void;
```

new:

```tsx
  onAdd: (type: ClientCertificate['type']) => void;
```

The second hint (lines 109-111), old:

```tsx
        <p className='text-[11px] text-muted-foreground'>
          Relative paths start at the collection folder. Encrypted PEM keys need their passphrase.
        </p>
```

new:

```tsx
        <p className='text-[11px] text-muted-foreground'>
          Relative paths start at the collection folder. Encrypted PEM keys need their passphrase.
        </p>
        <p className='text-[11px] text-muted-foreground'>
          A RocketVault certificate is exported when a request needs it and is never saved.
        </p>
```

The empty-state text (line 118), old:

```tsx
            Add a PEM or PKCS12 certificate to present it to matching hosts.
```

new:

```tsx
            Add a PEM, PKCS12 or RocketVault certificate to present it to matching hosts.
```

Pass the bindings to each row. In the `certificates.map`, old:

```tsx
                cert={cert}
                options={options}
                onChange={onChange}
```

new:

```tsx
                cert={cert}
                bindings={bindings}
                options={options}
                onChange={onChange}
```

In the footer, after the "Add PKCS12" `Button` (which ends at line 161), add:

```tsx
          <Button
            variant='ghost'
            size='sm'
            onClick={() => onAdd('vault')}
            className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
          >
            <Plus className='h-3.5 w-3.5' />
            Add RocketVault certificate
          </Button>
```

`CertificateRowProps` (lines 183-192), old:

```tsx
interface CertificateRowProps {
  idx: number;
  total: number;
  cert: ClientCertificate;
  options: VaultSecretOption[];
```

new:

```tsx
interface CertificateRowProps {
  idx: number;
  total: number;
  cert: ClientCertificate;
  bindings: ExternalSecretBinding[];
  options: VaultSecretOption[];
```

`CertificateRow`'s parameters (lines 194-203), old:

```tsx
function CertificateRow({
  idx,
  total,
  cert,
  options,
```

new:

```tsx
function CertificateRow({
  idx,
  total,
  cert,
  bindings,
  options,
```

The body D1 narrowed, old:

```tsx
      {cert.type !== 'vault' && (
        <>
```

new:

```tsx
      {cert.type === 'vault' ? (
        <VaultCertificateRow idx={idx} cert={cert} bindings={bindings} onChange={onChange} />
      ) : (
        <>
```

- [ ] **Step 4: Run the tab tests to verify they pass**

Run: `yarn vitest run src/components/environments/CertificatesTab.test.tsx`
Expected: PASS, the existing tests and the 2 new ones.

- [ ] **Step 5: Write the failing dialog tests**

In `src/components/environments/EnvironmentDialog.test.tsx`, the import line 9, old:

```tsx
import type { Environment } from '@/lib/tauri-api';
```

new:

```tsx
import type { ClientCertificate, Environment } from '@/lib/tauri-api';
```

In the `vi.mock('@/lib/tauri-api', ...)` factory (lines 13-24), old:

```tsx
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
  };
});
```

new:

```tsx
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
    listVaultCertificates: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));
```

Add inside `describe('EnvironmentDialog certificates tab', ...)`, before its closing `});` (line 329):

```tsx

  it('saves a RocketVault certificate unchanged', async () => {
    const vaultCert: ClientCertificate = {
      type: 'vault',
      domain: 'api.example.com',
      binding: 'vault',
      certificate: 'client-a',
      format: 'pkcs12',
    };
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([
      { ...prodEnv, externalSecrets: [vaultBinding], clientCertificates: [vaultCert] },
    ]);
    renderDialog();
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, savedEnv] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(savedEnv.clientCertificates).toEqual([vaultCert]);
  });

  it('blocks the save when a RocketVault certificate names a binding the environment lacks', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([
      {
        ...prodEnv,
        externalSecrets: [vaultBinding],
        clientCertificates: [
          { type: 'vault', domain: 'a.com', binding: 'payments', certificate: 'client-a' },
        ],
      },
    ]);
    renderDialog();
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveEnvironment).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('binding "payments"'));
  });

  it('adds a RocketVault certificate with PEM as its format', async () => {
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.click(screen.getByRole('tab', { name: /certificates/i }));
    await user.click(await screen.findByRole('button', { name: 'Add RocketVault certificate' }));

    expect(screen.getByRole('combobox', { name: 'Format for certificate 1' })).toHaveTextContent(
      'PEM',
    );
    expect(screen.getByRole('combobox', { name: 'Binding for certificate 1' })).toBeInTheDocument();
  });
```

- [ ] **Step 6: Run them to verify they fail**

Run: `yarn vitest run src/components/environments/EnvironmentDialog.test.tsx`
Expected: the two save tests PASS already (D1's validation is wired through `handleSave`); `adds a RocketVault certificate with PEM as its format` FAILS because `addClientCertificate` builds a PKCS12 entry for any type that is not `pem`, so no format picker appears.

- [ ] **Step 7: Create an empty vault entry in the dialog**

In `src/components/environments/EnvironmentDialog.tsx`, above `export function EnvironmentDialog` (find it by name), add:

```tsx
// A new, empty certificate of the given type. A vault entry writes its PEM default out.
function emptyCertificate(type: ClientCertificate['type']): ClientCertificate {
  switch (type) {
    case 'pem':
      return { type: 'pem', domain: '', certificateFilePath: '', privateKeyFilePath: '' };
    case 'pkcs12':
      return { type: 'pkcs12', domain: '', pkcs12FilePath: '' };
    case 'vault':
      return { type: 'vault', domain: '', binding: '', certificate: '', format: 'pem' };
  }
}
```

`addClientCertificate` (lines 331-346), old:

```tsx
  const addClientCertificate = useCallback(
    (type: 'pem' | 'pkcs12') => {
      if (!selectedEnv) return;
      const fresh: ClientCertificate =
        type === 'pem'
          ? { type: 'pem', domain: '', certificateFilePath: '', privateKeyFilePath: '' }
          : { type: 'pkcs12', domain: '', pkcs12FilePath: '' };
      setLocalEnvs((prev) =>
```

new:

```tsx
  const addClientCertificate = useCallback(
    (type: ClientCertificate['type']) => {
      if (!selectedEnv) return;
      const fresh = emptyCertificate(type);
      setLocalEnvs((prev) =>
```

- [ ] **Step 8: Run the frontend checks**

Run: `yarn vitest run src/components/environments/EnvironmentDialog.test.tsx src/components/environments/CertificatesTab.test.tsx src/components/environments/VaultCertificateRow.test.tsx src/lib/certificate-validation.test.ts src/lib/vault-certificates.test.ts`
Expected: PASS, all tests in the five files.

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no Biome errors in the touched files. If it reports formatting only, run `yarn format` and re-run the tests; never reformat files this task does not list.

- [ ] **Step 9: Manual check in the app**

Run: `yarn tauri dev`. In an environment with an External Secrets binding to a RocketVault v4 (the instance from the `rocketvault-4b` session), open Environments, then Certificates, then "Add RocketVault certificate". Check: the binding list shows the environment's aliases; the certificate list loads, shows key algorithms, and greys out non-exportable certificates; PEM is the default format; Save writes `type: vault` with names only into the environment YAML (open the file); a request to the domain presents the certificate. Record what was checked in the task report. If no live instance is available, say so in the report instead of claiming the check.

- [ ] **Step 10: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add src/components/environments/CertificatesTab.tsx src/components/environments/CertificatesTab.test.tsx src/components/environments/EnvironmentDialog.tsx src/components/environments/EnvironmentDialog.test.tsx
git commit -- src/components/environments/CertificatesTab.tsx src/components/environments/CertificatesTab.test.tsx src/components/environments/EnvironmentDialog.tsx src/components/environments/EnvironmentDialog.test.tsx
```

Suggested subject: `feat(ui): add RocketVault certificates to the Certificates tab`. The message ends with `Relates to: #21`.

---

## Milestone Checklist: Plan D

- [ ] `ClientCertificate` has the `vault` member; `VaultCertificateFormat` is `'pem' | 'pkcs12'`
- [ ] `validateClientCertificates` mirrors A2: bound alias, certificate name, known format, no key text in any vault field, no echo of key text
- [ ] The picker loads the certificates of the selected binding, shows key algorithms, disables non-exportable and disabled certificates, marks a stored name that is gone as "not found", and keeps a stale binding visible
- [ ] An EC certificate with PEM on Windows shows the PKCS12 warning, and nowhere else
- [ ] "Add RocketVault certificate" creates `{ type: 'vault', domain: '', binding: '', certificate: '', format: 'pem' }`; Save validates it and writes names only
- [ ] `yarn tsc --noEmit`, `yarn check` and the five test files pass

## Next Plan

None. This is the last plan of Plan E. After D3, run the `superpowers:finishing-a-development-branch` skill, and run the live export test (`cargo test -j4 -p rocket-infra live_rocketvault_export -- --ignored`) once the `rocketvault-4b` session's instance is available. Before release, confirm the list envelope and field names in `crates/rocket-infra/src/rocketvault/certificate_api.rs` against the published RocketVault v4 contract, and tell users that Rocket builds without the `vault` type hide an environment that holds one (index, "Spec versus code" item 1).
