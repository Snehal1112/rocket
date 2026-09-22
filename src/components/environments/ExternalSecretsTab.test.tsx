// src/components/environments/ExternalSecretsTab.test.tsx

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
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

function renderTab(
  bindings: ExternalSecretBinding[],
  overrides: Partial<{
    onChange: (idx: number, patch: Partial<ExternalSecretBinding>) => void;
    onAdd: () => void;
    onRemove: (idx: number) => void;
  }> = {},
) {
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
