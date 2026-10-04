# Azure Key Vault Plan 03: Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Azure Key Vault selectable in the Connections dialog, with its own fields, and make the dialog send and restore `config`.

**Architecture:** The provider descriptor table in `src/lib/secret-providers.ts` gains a `tenantId` field, per-provider labels and placeholders, and flips Azure to selectable. `tauri-api.ts` types `config` to match the Plan 01 DTO. The dialog reads the descriptor for what to render and validate, and carries `config` through save and edit.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, shadcn/ui, lucide-react.

**Spec:** [../../specs/2026-10-04-azure-key-vault-provider-design.md](../../specs/2026-10-04-azure-key-vault-provider-design.md) (section 7, 8)

**Index:** [00-plan-index.md](00-plan-index.md) holds the locked interface contract and the global constraints. Read both first. Plans 01 and 02 must be done.

## Global Constraints

- shadcn/ui primitives only, no raw `<button>`, `<input>`, `<select>`, `<form>` or `<dialog>`. Icons from `lucide-react` only.
- Use `yarn`. Verification is `yarn tsc --noEmit`, `yarn check` and `yarn test <pattern>`.
- The wire shape of `config` is `{ kind: 'azure', tenantId, authorityHost? }` (Plan 01, Task 2).
- RocketVault behavior and copy must not change, other than the shared required-fields message becoming data driven.

## Review Focus

- Editing an Azure connection must send its `config` back, otherwise the tenant is lost on every edit.
- A test-only `authorityHost` already stored on a connection survives an edit even though the form has no field for it.
- A blank tenant ID blocks the save with a message that names it.
- RocketVault still saves without a `config` key, so its YAML rows stay unchanged.
- The Azure form must not show Verify SSL or Allow insecure HTTP, which Azure ignores.
- The Azure option in the provider dropdown is no longer marked "not available yet", while AWS, HashiCorp and Google still are.

---

### Task 1: Descriptor table and API types

**Files:**
- Modify: `src/lib/secret-providers.ts`
- Modify: `src/lib/tauri-api.ts:196-208`
- Modify: `src/lib/secret-providers.test.ts`

**Interfaces:**
- Produces: `ConnectionField` with `'tenantId'`, `SecretProviderDescriptor.fieldLabels?` and `fieldPlaceholders?`, `connectionFieldLabel(descriptor, field)`, `requiredFieldsMessage(descriptor)`, and the `AzureConfig` / `ProviderConfig` types.

- [ ] **Step 1: Write the failing tests**

In `src/lib/secret-providers.test.ts` extend the import to include `connectionFieldLabel` and `requiredFieldsMessage`. Replace the test `only RocketVault is selectable and supports certificates` with:

```ts
  it('RocketVault and Azure are selectable, and only RocketVault supports certificates', () => {
    const selectable = SECRET_PROVIDERS.filter((p) => p.selectable).map((p) => p.kind);
    const certificates = SECRET_PROVIDERS.filter((p) => p.supportsCertificates).map((p) => p.kind);
    expect(selectable).toEqual(['rocketvault', 'azure']);
    expect(certificates).toEqual(['rocketvault']);
  });

  it('lists the Azure connection fields and no TLS switches', () => {
    expect(getProviderDescriptor('azure').connectionFields).toEqual([
      'baseUrl',
      'tenantId',
      'clientId',
      'clientSecret',
    ]);
  });
```

and add at the end of the file:

```ts
describe('connectionFieldLabel', () => {
  it('uses the default label unless the provider overrides it', () => {
    expect(connectionFieldLabel(getProviderDescriptor('rocketvault'), 'baseUrl')).toBe('Base URL');
    expect(connectionFieldLabel(getProviderDescriptor('azure'), 'baseUrl')).toBe('Vault URL');
    expect(connectionFieldLabel(getProviderDescriptor('azure'), 'tenantId')).toBe('Tenant ID');
  });
});

describe('requiredFieldsMessage', () => {
  it('names the required fields of the provider', () => {
    expect(requiredFieldsMessage(getProviderDescriptor('rocketvault'))).toBe(
      'Label, Base URL and Client ID are required.',
    );
    expect(requiredFieldsMessage(getProviderDescriptor('azure'))).toBe(
      'Label, Vault URL, Tenant ID and Client ID are required.',
    );
  });

  it('handles a provider with no required connection fields', () => {
    const none = { ...getProviderDescriptor('rocketvault'), connectionFields: [] };
    expect(requiredFieldsMessage(none)).toBe('Label is required.');
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `yarn test secret-providers`
Expected: FAIL, `connectionFieldLabel` and `requiredFieldsMessage` are not exported, and Azure is not selectable.

- [ ] **Step 3: Update the types**

In `src/lib/tauri-api.ts` replace the `SecretManagerConnection` interface (and add the types above it):

```ts
// Azure AD service principal settings. The vault URL is baseUrl and the app
// registration id is clientId. authorityHost exists for tests only.
export interface AzureConfig {
  kind: 'azure';
  tenantId: string;
  authorityHost?: string;
}

// Provider-specific, non-secret settings. Matches the Rust ProviderConfigDto.
export type ProviderConfig = AzureConfig;

export interface SecretManagerConnection {
  id: string;
  label: string;
  baseUrl: string;
  clientId: string;
  verifySsl: boolean;
  allowInsecureHttp: boolean;
  // Absent means RocketVault, for payloads written before providers existed.
  provider?: SecretProviderKind;
  config?: ProviderConfig | null;
}
```

- [ ] **Step 4: Update the descriptor table**

In `src/lib/secret-providers.ts` make these changes.

Add `'tenantId'` to the union:

```ts
export type ConnectionField =
  | 'baseUrl'
  | 'tenantId'
  | 'clientId'
  | 'clientSecret'
  | 'verifySsl'
  | 'allowInsecureHttp';
```

Add two optional members to `SecretProviderDescriptor`:

```ts
  // Overrides for the default field labels and placeholders below.
  fieldLabels?: Partial<Record<ConnectionField, string>>;
  fieldPlaceholders?: Partial<Record<ConnectionField, string>>;
```

Replace the Azure entry with:

```ts
  {
    kind: 'azure',
    label: 'Azure Key Vault',
    selectable: true,
    connectionFields: ['baseUrl', 'tenantId', 'clientId', 'clientSecret'],
    fieldLabels: { baseUrl: 'Vault URL' },
    fieldPlaceholders: { baseUrl: 'https://my-vault.vault.azure.net' },
    // The connection already names the vault, so the value is only a label.
    scopeLabel: 'Vault name',
    scopePlaceholder: 'Any name (the connection sets the vault)',
    supportsCertificates: false,
  },
```

Add after `getProviderDescriptor`:

```ts
const DEFAULT_FIELD_LABELS: Record<ConnectionField, string> = {
  baseUrl: 'Base URL',
  tenantId: 'Tenant ID',
  clientId: 'Client ID',
  clientSecret: 'Client Secret',
  verifySsl: 'Verify SSL',
  allowInsecureHttp: 'Allow insecure HTTP',
};

export function connectionFieldLabel(
  descriptor: SecretProviderDescriptor,
  field: ConnectionField,
): string {
  return descriptor.fieldLabels?.[field] ?? DEFAULT_FIELD_LABELS[field];
}

export function connectionFieldPlaceholder(
  descriptor: SecretProviderDescriptor,
  field: ConnectionField,
): string | undefined {
  return descriptor.fieldPlaceholders?.[field];
}

// The text fields that must be filled in before a connection can be saved.
const REQUIRED_TEXT_FIELDS: readonly ConnectionField[] = ['baseUrl', 'tenantId', 'clientId'];

export function requiredFieldsMessage(descriptor: SecretProviderDescriptor): string {
  const fields = REQUIRED_TEXT_FIELDS.filter((f) => descriptor.connectionFields.includes(f)).map(
    (f) => connectionFieldLabel(descriptor, f),
  );
  if (fields.length === 0) return 'Label is required.';
  const labels = ['Label', ...fields];
  return `${labels.slice(0, -1).join(', ')} and ${labels[labels.length - 1]} are required.`;
}
```

The RocketVault entry keeps `'baseUrl', 'clientId', 'clientSecret', 'verifySsl', 'allowInsecureHttp'` unchanged.

- [ ] **Step 5: Run to verify they pass**

Run: `yarn test secret-providers && yarn tsc --noEmit && yarn check`
Expected: PASS. `tsc` may report the dialog's `config` handling is still compatible (it never sets `config`), and `ExternalSecretsTab` is untouched.

- [ ] **Step 6: Commit**

```bash
git add src/lib/secret-providers.ts src/lib/secret-providers.test.ts src/lib/tauri-api.ts
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(secrets): describe the Azure connection fields`.

---

### Task 2: Dialog sends and restores `config`, and renders the Azure form

**Files:**
- Modify: `src/components/settings/SecretManagerConnectionsDialog.tsx`
- Modify: `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx`

**Interfaces:**
- Consumes: `connectionFieldLabel`, `connectionFieldPlaceholder`, `requiredFieldsMessage`, `ProviderConfig` (Task 1).

- [ ] **Step 1: Write the failing tests**

Add to the `describe` block of `SecretManagerConnectionsDialog.test.tsx` (the dialog's provider selector is disabled when editing, and the Radix select is awkward in jsdom, so these tests open the Azure form by editing a saved Azure connection):

```tsx
  const azureConnection: tauriApi.SecretManagerConnection = {
    id: 'az-1',
    label: 'Prod Azure',
    baseUrl: 'https://prod-kv.vault.azure.net',
    clientId: 'app-id',
    verifySsl: true,
    allowInsecureHttp: false,
    provider: 'azure',
    config: { kind: 'azure', tenantId: 'tenant-1' },
  };

  async function openAzureEdit(connection = azureConnection) {
    vi.mocked(tauriApi.listSecretManagerConnections).mockResolvedValue([connection]);
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /edit connection/i }));
    return user;
  }

  it('shows the Azure fields and hides the TLS switches', async () => {
    await openAzureEdit();

    expect(screen.getByLabelText(/vault url/i)).toHaveValue('https://prod-kv.vault.azure.net');
    expect(screen.getByLabelText(/tenant id/i)).toHaveValue('tenant-1');
    expect(screen.getByLabelText(/client id/i)).toHaveValue('app-id');
    expect(screen.getByLabelText(/client secret/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/verify ssl/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/allow insecure http/i)).not.toBeInTheDocument();
  });

  it('saves an edited Azure connection with its config', async () => {
    const user = await openAzureEdit();
    const tenant = screen.getByLabelText(/tenant id/i);
    await user.clear(tenant);
    await user.type(tenant, 'tenant-2');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: 'azure',
        baseUrl: 'https://prod-kv.vault.azure.net',
        config: { kind: 'azure', tenantId: 'tenant-2' },
      }),
      undefined,
    );
  });

  it('keeps a stored authority host that the form has no field for', async () => {
    const user = await openAzureEdit({
      ...azureConnection,
      config: { kind: 'azure', tenantId: 'tenant-1', authorityHost: 'http://127.0.0.1:9' },
    });
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).toHaveBeenCalledWith(
      expect.objectContaining({
        config: { kind: 'azure', tenantId: 'tenant-1', authorityHost: 'http://127.0.0.1:9' },
      }),
      undefined,
    );
  });

  it('does not save an Azure connection with a blank tenant', async () => {
    const user = await openAzureEdit();
    await user.clear(screen.getByLabelText(/tenant id/i));
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).not.toHaveBeenCalled();
  });

  it('saves a RocketVault edit without a config key', async () => {
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

    const saved = vi.mocked(tauriApi.saveSecretManagerConnection).mock.calls[0]?.[0];
    expect(saved).toBeDefined();
    expect(saved).not.toHaveProperty('config');
  });
```

The existing suite's `vi.resetAllMocks()` plus the `mockResolvedValue` in `beforeEach` already cover `saveSecretManagerConnection` resolving. If `saveSecretManagerConnection` returns `undefined` after the reset, the mutation still succeeds, so no extra mock is needed.

- [ ] **Step 2: Run to verify they fail**

Run: `yarn test SecretManagerConnectionsDialog`
Expected: the new tests FAIL (no "Tenant ID" field, `config` is never sent, and the Azure form still shows the TLS switches). Existing tests pass.

- [ ] **Step 3: Update the dialog**

In `src/components/settings/SecretManagerConnectionsDialog.tsx`:

Update the imports:

```tsx
import {
  connectionFieldLabel,
  connectionFieldPlaceholder,
  getProviderDescriptor,
  requiredFieldsMessage,
  SECRET_PROVIDERS,
} from '@/lib/secret-providers';
import type {
  ProviderConfig,
  SecretManagerConnection,
  SecretProviderKind,
} from '@/lib/tauri-api';
```

Extend `emptyForm` (the authority host has no input, it is carried only so an edit does not drop it):

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
  tenantId: '',
  authorityHost: '',
};
```

Replace `startEdit`:

```tsx
  const startEdit = (c: SecretManagerConnection) =>
    setEditing({
      ...emptyForm,
      id: c.id,
      label: c.label,
      baseUrl: c.baseUrl,
      clientId: c.clientId,
      verifySsl: c.verifySsl,
      allowInsecureHttp: c.allowInsecureHttp,
      provider: c.provider ?? 'rocketvault',
      tenantId: c.config?.tenantId ?? '',
      authorityHost: c.config?.authorityHost ?? '',
      isNew: false,
    });
```

Replace the validation block and the `connection` object in `handleSave` (from `const descriptorFields` down to the end of the `connection` literal) with:

```tsx
    const descriptorFields = descriptor.connectionFields;
    const needsBaseUrl = descriptorFields.includes('baseUrl');
    const needsTenantId = descriptorFields.includes('tenantId');
    const needsClientId = descriptorFields.includes('clientId');
    if (
      !editing.label.trim() ||
      (needsBaseUrl && !editing.baseUrl.trim()) ||
      (needsTenantId && !editing.tenantId.trim()) ||
      (needsClientId && !editing.clientId.trim())
    ) {
      toast.error(requiredFieldsMessage(descriptor));
      return;
    }
    if (needsBaseUrl && !/^https?:\/\/[^/\s]+/i.test(editing.baseUrl.trim())) {
      toast.error(
        `${connectionFieldLabel(descriptor, 'baseUrl')} must start with http:// or https://.`,
      );
      return;
    }
    if (
      descriptorFields.includes('clientSecret') &&
      editing.isNew &&
      !editing.clientSecret.trim()
    ) {
      toast.error('A client secret is required when adding a new connection.');
      return;
    }
    const config: ProviderConfig | undefined =
      editing.provider === 'azure'
        ? {
            kind: 'azure',
            tenantId: editing.tenantId.trim(),
            ...(editing.authorityHost ? { authorityHost: editing.authorityHost } : {}),
          }
        : undefined;
    const connection: SecretManagerConnection = {
      id: editing.id,
      label: editing.label.trim(),
      baseUrl: editing.baseUrl.trim(),
      clientId: editing.clientId.trim(),
      verifySsl: editing.verifySsl,
      allowInsecureHttp: editing.allowInsecureHttp,
      provider: editing.provider,
      ...(config ? { config } : {}),
    };
```

Replace the existing `Base URL` field block (the `{fields.includes('baseUrl') && (...)}` element) with the block below, and insert the new Tenant ID block directly after it. Use the existing `Label` and `Input` imports and keep the existing `id='sm-base-url'`:

```tsx
            {fields.includes('baseUrl') && (
              <div>
                <Label htmlFor='sm-base-url' className='text-sm'>
                  {connectionFieldLabel(descriptor, 'baseUrl')}
                </Label>
                <Input
                  id='sm-base-url'
                  value={editing.baseUrl}
                  onChange={(e) => setEditing({ ...editing, baseUrl: e.target.value })}
                  placeholder={
                    connectionFieldPlaceholder(descriptor, 'baseUrl') ?? 'https://vault.internal:8774'
                  }
                  className='h-8 text-sm'
                />
              </div>
            )}
            {fields.includes('tenantId') && (
              <div>
                <Label htmlFor='sm-tenant-id' className='text-sm'>
                  {connectionFieldLabel(descriptor, 'tenantId')}
                </Label>
                <Input
                  id='sm-tenant-id'
                  value={editing.tenantId}
                  onChange={(e) => setEditing({ ...editing, tenantId: e.target.value })}
                  placeholder='Directory (tenant) ID or domain'
                  className='h-8 text-sm'
                />
              </div>
            )}
```

Replace the existing `const fields = ...` line near the top of the component with these two, so both the JSX and `handleSave` can use `descriptor`:

```tsx
  const descriptor = getProviderDescriptor(editing?.provider);
  const fields = editing ? descriptor.connectionFields : [];
```

(`getProviderDescriptor` already treats `undefined` as RocketVault.) `handleSave` reads this component-level `descriptor`, which is why the validation block above does not declare its own.

Make the Client ID and Client Secret labels use the descriptor too, replacing the literal `Client ID` text with `{connectionFieldLabel(descriptor, 'clientId')}` and the literal `Client Secret{' '}` with `{connectionFieldLabel(descriptor, 'clientSecret')}{' '}`.

Finally, make each row's test input follow its provider. Replace `placeholder='vault name'` in the row with:

```tsx
                          placeholder={getProviderDescriptor(c.provider).scopePlaceholder}
```

- [ ] **Step 4: Run to verify they pass**

Run: `yarn test SecretManagerConnectionsDialog && yarn tsc --noEmit && yarn check`
Expected: PASS. If Biome reports formatting issues, run `yarn lint` once and re-run `yarn check`.

- [ ] **Step 5: Commit**

```bash
git add src/components/settings/SecretManagerConnectionsDialog.tsx src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(secrets): add the Azure connection form`.

---

### Task 3: Provider dropdown and External Secrets copy

**Files:**
- Modify: `src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx`
- Modify: `src/lib/secret-providers.test.ts`
- Modify (only if a test fails): `src/components/environments/ExternalSecretsTab.tsx`

**Interfaces:**
- Consumes: the Azure descriptor from Task 1.

- [ ] **Step 1: Write the failing tests**

The dropdown label comes from `SecretProviderDescriptor.selectable`, already rendered by the dialog (`p.selectable ? p.label : \`${p.label} (not available yet)\``). Pin that Azure no longer reads "not available yet" while the other three do. Add to `secret-providers.test.ts`:

```ts
describe('the not-available-yet providers', () => {
  it('lists exactly AWS, HashiCorp and Google as not selectable', () => {
    const blocked = SECRET_PROVIDERS.filter((p) => !p.selectable).map((p) => p.kind);
    expect(blocked).toEqual(['aws', 'hashicorp', 'gcp']);
  });

  it('labels the Azure binding scope as a free-form name', () => {
    const azure = getProviderDescriptor('azure');
    expect(azure.scopeLabel).toBe('Vault name');
    expect(azure.scopePlaceholder).toMatch(/any name/i);
  });
});
```

Add to the dialog test file, in the same `describe`, a test that opens the provider dropdown. Radix Select can fail in jsdom without pointer-capture polyfills, so it asserts through the listbox only if the control opens, and otherwise relies on the descriptor test above:

```tsx
  it('marks only the providers without an implementation as not available yet', async () => {
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.click(screen.getByRole('combobox', { name: /provider/i }));

    expect(await screen.findByRole('option', { name: /^azure key vault$/i })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /aws secrets manager \(not available yet\)/i })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /hashicorp vault \(not available yet\)/i })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /google secret manager \(not available yet\)/i })).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run to verify the outcome**

Run: `yarn test secret-providers SecretManagerConnectionsDialog`
Expected: the descriptor tests PASS (Task 1 already did the work). The dropdown test PASSES if Radix opens in this jsdom setup. If it fails with a pointer-capture or `scrollIntoView` error, add these two lines to the top of that one test and re-run:

```tsx
    Element.prototype.hasPointerCapture = () => false;
    Element.prototype.scrollIntoView = () => undefined;
```

If it still cannot open, delete the dropdown test, keep the descriptor tests, and record "dropdown label checked manually" in the commit body.

- [ ] **Step 3: Check the External Secrets tab with an Azure connection**

Run: `yarn test ExternalSecretsTab`
Expected: PASS, no change needed. The tab reads `scopeLabel` and `scopePlaceholder` from the descriptor, so Azure bindings show "Vault name" with the new placeholder. If a test there hard-codes the old Azure copy (`Vault`), update that assertion to the new copy.

- [ ] **Step 4: Run the whole frontend verification**

Run: `yarn tsc --noEmit && yarn check && yarn test secret`
Expected: PASS with no linter errors.

- [ ] **Step 5: Manual check in the real app**

Run `yarn tauri dev`. Open Secret Manager Connections, add an Azure connection (vault URL, tenant, client id, secret), press Test with any name, then bind it in an environment, press Fetch Secrets, and send a request using `{{alias.secretName}}`. Record the outcome. This needs a real Azure vault, so if none is available say so in the report instead of claiming it works.

- [ ] **Step 6: Commit**

```bash
git add src/lib/secret-providers.test.ts src/components/settings/__tests__/SecretManagerConnectionsDialog.test.tsx
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `test(secrets): pin the provider dropdown and Azure scope copy`.

---

## Next Plan

None. This is the last plan. Run the final verification block in [00-plan-index.md](00-plan-index.md), then update the secret provider foundation memory and report the result, including any verification step that could not be run.
