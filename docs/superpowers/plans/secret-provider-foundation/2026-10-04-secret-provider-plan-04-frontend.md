# Secret provider foundation, Plan 04: Frontend provider selector and certificate gating

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the UI carry a connection's provider, drive the connection form and the External Secrets labels from a per-provider descriptor, and hide the certificate button when no binding can supply certificates.

**Architecture:** A `src/lib/secret-providers.ts` descriptor table is the single place that says what each provider looks like in the UI. The connections dialog, the External Secrets tab and the Environment dialog read it. Only RocketVault is selectable in this foundation, and a new provider adds one descriptor entry and no new branches.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, shadcn/ui, Yarn.

**Spec:** [../../specs/2026-10-04-secret-provider-foundation-design.md](../../specs/2026-10-04-secret-provider-foundation-design.md) (sections 7, 8)

**Depends on:** [Plan 02](2026-10-04-secret-provider-plan-02-dispatch-and-service.md) for the IPC shape.

## Global Constraints

- Use shadcn/ui primitives only (no raw `<button>`, `<input>`, `<select>`, `<dialog>`, `<form>`).
- Icons come from `lucide-react` only.
- Never fully destructure Zustand store state at component top level. (No store is touched here.)
- Use Yarn: `yarn test <pattern>`, `yarn tsc --noEmit`, `yarn check`.
- `provider` is optional on the TypeScript connection type, because the existing test fixtures omit it and an absent value means RocketVault.
- Commit with conventional commits, using the `dev-workflow-skills:1-git-commit` skill, staging by explicit path only. Never stage `crates/rocket-app/src/execution_service.rs`.

## Review Focus

- A connection with no `provider` (an older payload or fixture) must behave as RocketVault everywhere (Task 1 tests).
- The dialog must still require a client secret for a new RocketVault connection, and the existing dialog tests must pass unchanged (Task 2).
- The provider of an existing connection must not be editable, so its stored credential is never orphaned (Task 2).
- While connections are still loading, the certificate button must stay visible, not flicker away (Task 1 and Task 3 tests).
- The `Vault name for binding N` aria label must not change, because existing tests and assistive technology rely on it (Task 3).

---

## Task 1: Provider types and the descriptor module

**Files:**
- Modify: `src/lib/tauri-api.ts` (the `SecretManagerConnection` interface at about line 196)
- Create: `src/lib/secret-providers.ts`
- Create: `src/lib/secret-providers.test.ts`

**Interfaces:**
- Produces:
  - `SecretProviderKind` type (`'rocketvault' | 'azure' | 'aws' | 'hashicorp' | 'gcp'`) exported from `src/lib/tauri-api.ts`.
  - `SecretManagerConnection.provider?: SecretProviderKind` and `SecretManagerConnection.config?: Record<string, unknown> | null`.
  - From `src/lib/secret-providers.ts`: `ConnectionField`, `SecretProviderDescriptor`, `SECRET_PROVIDERS`, `getProviderDescriptor(kind?)`, `bindingScopeColumnLabel(bindings, connections)`, `canAddVaultCertificate(bindings, connections, connectionsLoaded)`.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/secret-providers.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  bindingScopeColumnLabel,
  canAddVaultCertificate,
  getProviderDescriptor,
  SECRET_PROVIDERS,
} from '@/lib/secret-providers';
import type { ExternalSecretBinding, SecretManagerConnection } from '@/lib/tauri-api';

const rocketVault: SecretManagerConnection = {
  id: 'rv',
  label: 'Prod RocketVault',
  baseUrl: 'https://vault.internal:8774',
  clientId: 'rocketapi',
  verifySsl: true,
  allowInsecureHttp: false,
};

const azure: SecretManagerConnection = { ...rocketVault, id: 'az', provider: 'azure' };

function binding(connectionId: string): ExternalSecretBinding {
  return { alias: `a-${connectionId}`, connectionId, vaultName: 'v', secretNames: [] };
}

describe('getProviderDescriptor', () => {
  it('treats a missing provider as RocketVault', () => {
    expect(getProviderDescriptor(undefined).kind).toBe('rocketvault');
    expect(getProviderDescriptor(null).kind).toBe('rocketvault');
  });

  it('only RocketVault is selectable and supports certificates', () => {
    const selectable = SECRET_PROVIDERS.filter((p) => p.selectable).map((p) => p.kind);
    const certificates = SECRET_PROVIDERS.filter((p) => p.supportsCertificates).map((p) => p.kind);
    expect(selectable).toEqual(['rocketvault']);
    expect(certificates).toEqual(['rocketvault']);
  });

  it('lists the RocketVault connection fields the form shows today', () => {
    expect(getProviderDescriptor('rocketvault').connectionFields).toEqual([
      'baseUrl',
      'clientId',
      'clientSecret',
      'verifySsl',
      'allowInsecureHttp',
    ]);
  });
});

describe('bindingScopeColumnLabel', () => {
  it('says Vault Name when there are no bindings or only RocketVault ones', () => {
    expect(bindingScopeColumnLabel([], [rocketVault])).toBe('Vault Name');
    expect(bindingScopeColumnLabel([binding('rv')], [rocketVault])).toBe('Vault Name');
  });

  it('says Scope when a binding uses another provider', () => {
    expect(bindingScopeColumnLabel([binding('rv'), binding('az')], [rocketVault, azure])).toBe(
      'Scope',
    );
  });
});

describe('canAddVaultCertificate', () => {
  it('stays true while connections are still loading', () => {
    expect(canAddVaultCertificate([], [], false)).toBe(true);
  });

  it('is false with no binding once connections have loaded', () => {
    expect(canAddVaultCertificate([], [rocketVault], true)).toBe(false);
  });

  it('is true when a binding points at a RocketVault connection', () => {
    expect(canAddVaultCertificate([binding('rv')], [rocketVault], true)).toBe(true);
  });

  it('is false when every binding points at a provider without certificates', () => {
    expect(canAddVaultCertificate([binding('az')], [azure], true)).toBe(false);
  });

  it('ignores a binding whose connection was deleted', () => {
    expect(canAddVaultCertificate([binding('gone')], [rocketVault], true)).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test src/lib/secret-providers.test.ts --run`
Expected: FAIL (module `@/lib/secret-providers` not found).

- [ ] **Step 3: Add the types**

In `src/lib/tauri-api.ts`, replace the `SecretManagerConnection` interface with:

```ts
export type SecretProviderKind = 'rocketvault' | 'azure' | 'aws' | 'hashicorp' | 'gcp';

export interface SecretManagerConnection {
  id: string;
  label: string;
  baseUrl: string;
  clientId: string;
  verifySsl: boolean;
  allowInsecureHttp: boolean;
  // Absent means RocketVault, for payloads written before providers existed.
  provider?: SecretProviderKind;
  // Provider-specific, non-secret settings. No provider uses it yet.
  config?: Record<string, unknown> | null;
}
```

- [ ] **Step 4: Implement the descriptor module**

Create `src/lib/secret-providers.ts`:

```ts
import type {
  ExternalSecretBinding,
  SecretManagerConnection,
  SecretProviderKind,
} from '@/lib/tauri-api';

// The form fields a provider's connection needs. Each provider's own work adds
// its entry here, and the connection dialog renders from this list.
export type ConnectionField =
  | 'baseUrl'
  | 'clientId'
  | 'clientSecret'
  | 'verifySsl'
  | 'allowInsecureHttp';

export interface SecretProviderDescriptor {
  kind: SecretProviderKind;
  label: string;
  // False until the provider's own implementation ships.
  selectable: boolean;
  connectionFields: readonly ConnectionField[];
  // What a binding's scope field means for this provider.
  scopeLabel: string;
  scopePlaceholder: string;
  supportsCertificates: boolean;
}

export const SECRET_PROVIDERS: readonly SecretProviderDescriptor[] = [
  {
    kind: 'rocketvault',
    label: 'RocketVault',
    selectable: true,
    connectionFields: ['baseUrl', 'clientId', 'clientSecret', 'verifySsl', 'allowInsecureHttp'],
    scopeLabel: 'Vault Name',
    scopePlaceholder: 'Vault name',
    supportsCertificates: true,
  },
  {
    kind: 'azure',
    label: 'Azure Key Vault',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Vault',
    scopePlaceholder: 'Vault',
    supportsCertificates: false,
  },
  {
    kind: 'aws',
    label: 'AWS Secrets Manager',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Region',
    scopePlaceholder: 'Region',
    supportsCertificates: false,
  },
  {
    kind: 'hashicorp',
    label: 'HashiCorp Vault',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Mount',
    scopePlaceholder: 'Mount',
    supportsCertificates: false,
  },
  {
    kind: 'gcp',
    label: 'Google Secret Manager',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Project',
    scopePlaceholder: 'Project',
    supportsCertificates: false,
  },
];

const ROCKETVAULT = SECRET_PROVIDERS[0] as SecretProviderDescriptor;

// An absent provider is a RocketVault connection from before providers existed.
export function getProviderDescriptor(kind?: SecretProviderKind | null): SecretProviderDescriptor {
  return SECRET_PROVIDERS.find((p) => p.kind === kind) ?? ROCKETVAULT;
}

function connectionOf(
  binding: ExternalSecretBinding,
  connections: SecretManagerConnection[],
): SecretManagerConnection | undefined {
  return connections.find((c) => c.id === binding.connectionId);
}

// The External Secrets column header. RocketVault calls it a vault name and the
// other providers use different words, so a mixed list falls back to "Scope".
export function bindingScopeColumnLabel(
  bindings: ExternalSecretBinding[],
  connections: SecretManagerConnection[],
): string {
  const labels = new Set(
    bindings.map((b) => getProviderDescriptor(connectionOf(b, connections)?.provider).scopeLabel),
  );
  if (labels.size === 0) return ROCKETVAULT.scopeLabel;
  if (labels.size === 1) return [...labels][0] as string;
  return 'Scope';
}

// A vault certificate needs a binding whose connection can supply certificates.
// While connections are loading the answer is true, so the button does not flicker.
export function canAddVaultCertificate(
  bindings: ExternalSecretBinding[],
  connections: SecretManagerConnection[],
  connectionsLoaded: boolean,
): boolean {
  if (!connectionsLoaded) return true;
  return bindings.some((b) => {
    const connection = connectionOf(b, connections);
    return !!connection && getProviderDescriptor(connection.provider).supportsCertificates;
  });
}
```

- [ ] **Step 5: Run the tests and the type check**

Run: `yarn test src/lib/secret-providers.test.ts --run`
Expected: PASS (11 tests).
Run: `yarn tsc --noEmit`
Expected: PASS.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add src/lib/tauri-api.ts src/lib/secret-providers.ts src/lib/secret-providers.test.ts
```

Suggested subject: `feat(secrets): add the provider descriptor table`.

---

## Task 2: Provider selector in the connections dialog

**Files:**
- Modify: `src/components/settings/SecretManagerConnectionsDialog.tsx`
- Modify: `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx`

**Interfaces:**
- Consumes: `SECRET_PROVIDERS`, `getProviderDescriptor` (Task 1), the `Select` primitives from `@/components/ui/select`.
- Produces: the saved connection now includes `provider`. The form shows only the fields the provider's descriptor lists.

- [ ] **Step 1: Write the failing tests**

Add to the `describe('SecretManagerConnectionsDialog', ...)` block of `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx`:

```tsx
  it('shows a provider selector on the add form', async () => {
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));

    expect(screen.getByRole('combobox', { name: /provider/i })).toBeInTheDocument();
  });

  it('saves a new connection with provider rocketvault by default', async () => {
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.type(screen.getByLabelText(/^label$/i), 'Prod Vault');
    await user.type(screen.getByLabelText(/base url/i), 'https://vault.internal:8774');
    await user.type(screen.getByLabelText(/client id/i), 'rocketapi');
    await user.type(screen.getByLabelText(/client secret/i), 'super-secret-value');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).toHaveBeenCalledWith(
      expect.objectContaining({ provider: 'rocketvault' }),
      'super-secret-value',
    );
  });
```

- [ ] **Step 2: Run to verify they fail**

Run: `yarn test src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx --run`
Expected: the two new tests FAIL (no provider combobox, no `provider` in the payload). The existing tests pass.

- [ ] **Step 3: Add the provider to the form state and the payload**

In `src/components/settings/SecretManagerConnectionsDialog.tsx`:

1. Add imports:

```tsx
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { getProviderDescriptor, SECRET_PROVIDERS } from '@/lib/secret-providers';
import type { SecretManagerConnection, SecretProviderKind } from '@/lib/tauri-api';
```

If `SecretManagerConnection` is already imported from `@/lib/tauri-api`, merge `SecretProviderKind` into that import instead of adding a second one.

2. Add `provider` to `emptyForm`:

```tsx
const emptyForm = {
  id: '',
  label: '',
  baseUrl: '',
  clientId: '',
  verifySsl: true,
  allowInsecureHttp: false,
  clientSecret: '',
  provider: 'rocketvault' as SecretProviderKind,
};
```

3. Make `startEdit` default an absent provider:

```tsx
  const startEdit = (c: SecretManagerConnection) =>
    setEditing({ ...c, provider: c.provider ?? 'rocketvault', clientSecret: '', isNew: false });
```

4. Replace the validation and payload part of `handleSave` (from the first `if (!editing.label.trim() ...` through the `const connection ... };` block) with:

```tsx
    const fields = getProviderDescriptor(editing.provider).connectionFields;
    const needsBaseUrl = fields.includes('baseUrl');
    const needsClientId = fields.includes('clientId');
    if (
      !editing.label.trim() ||
      (needsBaseUrl && !editing.baseUrl.trim()) ||
      (needsClientId && !editing.clientId.trim())
    ) {
      toast.error('Label, base URL and client ID are required.');
      return;
    }
    if (needsBaseUrl && !/^https?:\/\/[^/\s]+/i.test(editing.baseUrl.trim())) {
      toast.error('Base URL must start with http:// or https://.');
      return;
    }
    if (fields.includes('clientSecret') && editing.isNew && !editing.clientSecret.trim()) {
      toast.error('A client secret is required when adding a new connection.');
      return;
    }
    const connection: SecretManagerConnection = {
      id: editing.id,
      label: editing.label.trim(),
      baseUrl: editing.baseUrl.trim(),
      clientId: editing.clientId.trim(),
      verifySsl: editing.verifySsl,
      allowInsecureHttp: editing.allowInsecureHttp,
      provider: editing.provider,
    };
```

- [ ] **Step 4: Render the selector and drive the fields from the descriptor**

Inside the edit form, compute the field list once near the top of the component body (after the `editing` state hooks):

```tsx
  const fields = editing ? getProviderDescriptor(editing.provider).connectionFields : [];
```

Immediately before the `<div>` that holds `<Label htmlFor='sm-base-url'`, insert the selector:

```tsx
            <div>
              <Label htmlFor='sm-provider' className='text-sm'>
                Provider
              </Label>
              <Select
                value={editing.provider}
                onValueChange={(value) =>
                  setEditing({ ...editing, provider: value as SecretProviderKind })
                }
                // A saved connection keeps its provider, so its stored credential stays valid.
                disabled={!editing.isNew}
              >
                <SelectTrigger id='sm-provider' className='h-8 text-sm'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SECRET_PROVIDERS.map((p) => (
                    <SelectItem key={p.kind} value={p.kind} disabled={!p.selectable}>
                      {p.selectable ? p.label : `${p.label} (not available yet)`}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
```

Wrap each existing field block in a descriptor check, keeping its inner JSX exactly as it is:
- the Base URL `<div>` in `{fields.includes('baseUrl') && ( ... )}`,
- the Client ID `<div>` in `{fields.includes('clientId') && ( ... )}`,
- the Client Secret `<div>` in `{fields.includes('clientSecret') && ( ... )}`,
- the Verify SSL `<div className='flex items-center justify-between'>` in `{fields.includes('verifySsl') && ( ... )}`,
- the Allow insecure HTTP `<div className='flex items-center justify-between'>` in `{fields.includes('allowInsecureHttp') && ( ... )}`.

- [ ] **Step 5: Run the tests**

Run: `yarn test src/components/settings --run`
Expected: PASS, including every pre-existing dialog test.
Run: `yarn tsc --noEmit && yarn check`
Expected: PASS. If Biome reports formatting, run `yarn format` on the touched files and re-run `yarn check`.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add src/components/settings/SecretManagerConnectionsDialog.tsx src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx
```

Suggested subject: `feat(secrets): add a provider selector to the connections dialog`.

---

## Task 3: Provider-aware External Secrets tab and certificate button

**Files:**
- Modify: `src/components/environments/ExternalSecretsTab.tsx`
- Modify: `src/components/environments/CertificatesTab.tsx`
- Modify: `src/components/environments/EnvironmentDialog.tsx`
- Modify: `src/components/environments/ExternalSecretsTab.test.tsx`
- Modify: `src/components/environments/CertificatesTab.test.tsx`

**Interfaces:**
- Consumes: `getProviderDescriptor`, `bindingScopeColumnLabel`, `canAddVaultCertificate` (Task 1).
- Produces: `CertificatesTabProps.canAddVaultCertificate?: boolean` (default `true`).

- [ ] **Step 1: Write the failing tests**

In `src/components/environments/CertificatesTab.test.tsx`, extend `renderTab` so a test can pass the new prop. Change its `overrides` type and the render call:

```tsx
function renderTab(
  certificates: ClientCertificate[],
  overrides: Partial<{
    isDirty: boolean;
    onSave: () => void;
    canAddVaultCertificate: boolean;
  }> = {},
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
      canAddVaultCertificate={overrides.canAddVaultCertificate}
    />,
  );
  return handlers;
}
```

Add inside `describe('CertificatesTab', ...)`:

```tsx
  it('shows the RocketVault certificate button by default', () => {
    renderTab([]);
    expect(screen.getByRole('button', { name: /add rocketvault certificate/i })).toBeInTheDocument();
  });

  it('hides the RocketVault certificate button when no binding can supply certificates', () => {
    renderTab([], { canAddVaultCertificate: false });
    expect(
      screen.queryByRole('button', { name: /add rocketvault certificate/i }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add PEM' })).toBeInTheDocument();
  });
```

In `src/components/environments/ExternalSecretsTab.test.tsx`, add:

```tsx
  it('keeps the Vault Name header and the vault name aria label for RocketVault bindings', async () => {
    vi.mocked(tauriApi.listSecretManagerConnections).mockResolvedValue([connection]);
    renderTab([binding]);

    expect(await screen.findByText('Vault Name')).toBeInTheDocument();
    expect(screen.getByLabelText('Vault name for binding 1')).toBeInTheDocument();
  });

  it('describes the binding without naming one provider when the list is empty', () => {
    renderTab([]);

    expect(screen.getByText(/bind a secret manager connection/i)).toBeInTheDocument();
  });
```

If `tauriApi` is not imported in that test file under that name, use the same mocked-module handle the file already uses for `listSecretManagerConnections`. Read the top 20 lines of the file to match it.

- [ ] **Step 2: Run to verify they fail**

Run: `yarn test src/components/environments/CertificatesTab.test.tsx src/components/environments/ExternalSecretsTab.test.tsx --run`
Expected: the hide-button test and the neutral-copy test FAIL. The default-button and header tests pass.

- [ ] **Step 3: Gate the certificate button**

In `src/components/environments/CertificatesTab.tsx`:

1. Add the optional prop to `CertificatesTabProps`:

```tsx
  // False hides "Add RocketVault certificate" when no binding can supply certificates.
  canAddVaultCertificate?: boolean;
```

2. Destructure it with a default in the component signature: add `canAddVaultCertificate = true,` after `variableContext,`.

3. Wrap the vault button (the `<Button ... onClick={() => onAdd('vault')} ...>Add RocketVault certificate</Button>` block) in `{canAddVaultCertificate && ( ... )}`.

In `src/components/environments/EnvironmentDialog.tsx`:

1. Add imports:

```tsx
import { canAddVaultCertificate } from '@/lib/secret-providers';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
```

2. Inside the component body, near the other query hooks, add:

```tsx
  const { data: secretConnections = [], isSuccess: secretConnectionsLoaded } =
    useSecretManagerConnections();
```

3. Pass the prop to `<CertificatesTab` (after `variableContext={variableContext}`):

```tsx
                    canAddVaultCertificate={canAddVaultCertificate(
                      selectedEnv.externalSecrets,
                      secretConnections,
                      secretConnectionsLoaded,
                    )}
```

- [ ] **Step 4: Make the External Secrets tab provider aware**

In `src/components/environments/ExternalSecretsTab.tsx`:

1. Add `import { bindingScopeColumnLabel, getProviderDescriptor } from '@/lib/secret-providers';`.

2. In `ExternalSecretsTab`, change the header text. Replace the `Vault Name` paragraph's text with `{bindingScopeColumnLabel(bindings, connections)}`:

```tsx
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          {bindingScopeColumnLabel(bindings, connections)}
        </p>
```

3. Replace the empty-state sentence `Bind a RocketVault connection and vault to fetch secret names for this environment.` with `Bind a secret manager connection and its vault to fetch secret names for this environment.`

4. In `BindingRow`, look up the binding's connection and use the descriptor's placeholder. After the `connectionMissing` constant add:

```tsx
  const scopePlaceholder = getProviderDescriptor(
    connections.find((c) => c.id === binding.connectionId)?.provider,
  ).scopePlaceholder;
```

and change the scope `<Input` from `placeholder='Vault name'` to `placeholder={scopePlaceholder}`. Leave `aria-label={`Vault name for binding ${idx + 1}`}` exactly as it is.

- [ ] **Step 5: Run the checks**

Run: `yarn test src/components/environments --run`
Expected: PASS, including every existing environment test (the dialog test wraps a `QueryClientProvider`, so the new hook is safe there).
Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add src/components/environments/ExternalSecretsTab.tsx src/components/environments/CertificatesTab.tsx src/components/environments/EnvironmentDialog.tsx src/components/environments/ExternalSecretsTab.test.tsx src/components/environments/CertificatesTab.test.tsx
```

Suggested subject: `feat(secrets): make the secrets and certificate tabs provider aware`.

---

## Done

This was the last plan of the foundation. Run the final checks once: `cargo check -j4 -p rocket --tests`, `yarn tsc --noEmit`, `yarn check`. Then each provider (Azure, AWS, HashiCorp, Google) gets its own brainstorm, spec and plan, starting from the descriptor entry and the `VaultSecretFetcher` implementation it needs to add.
