import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SecretManagerConnectionsDialog } from '@/components/settings/SecretManagerConnectionsDialog';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listSecretManagerConnections: vi.fn().mockResolvedValue([]),
    saveSecretManagerConnection: vi.fn().mockResolvedValue(undefined),
    deleteSecretManagerConnection: vi.fn().mockResolvedValue(undefined),
    testSecretManagerConnection: vi.fn().mockResolvedValue(undefined),
  };
});

function renderDialog() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <SecretManagerConnectionsDialog
        open
        // biome-ignore lint/suspicious/noEmptyBlockStatements: no-op stub for the test harness.
        onOpenChange={() => {}}
      />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(tauriApi.listSecretManagerConnections).mockResolvedValue([]);
});

describe('SecretManagerConnectionsDialog', () => {
  it('renders an empty state with no connections', async () => {
    renderDialog();
    expect(await screen.findByText(/no connections configured/i)).toBeInTheDocument();
  });

  it('adding a connection calls save with the right shape and the typed client secret', async () => {
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.type(screen.getByLabelText(/^label$/i), 'Prod Vault');
    await user.type(screen.getByLabelText(/base url/i), 'https://vault.internal:8774');
    await user.type(screen.getByLabelText(/client id/i), 'rocketapi');
    await user.type(screen.getByLabelText(/client secret/i), 'super-secret-value');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).toHaveBeenCalledWith(
      expect.objectContaining({ label: 'Prod Vault', baseUrl: 'https://vault.internal:8774' }),
      'super-secret-value',
    );
  });

  it('adding a connection without a client secret does not call save', async () => {
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.type(screen.getByLabelText(/^label$/i), 'Prod Vault');
    await user.type(screen.getByLabelText(/base url/i), 'https://vault.internal:8774');
    await user.type(screen.getByLabelText(/client id/i), 'rocketapi');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).not.toHaveBeenCalled();
  });

  it('editing an existing connection without touching the secret field saves with clientSecret undefined', async () => {
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

    expect(tauriApi.saveSecretManagerConnection).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'conn-1' }),
      undefined,
    );
  });

  it('adding a connection with a blank client ID does not call save', async () => {
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.type(screen.getByLabelText(/^label$/i), 'Prod Vault');
    await user.type(screen.getByLabelText(/base url/i), 'https://vault.internal:8774');
    await user.type(screen.getByLabelText(/client secret/i), 'super-secret-value');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveSecretManagerConnection).not.toHaveBeenCalled();
  });

  it('keeps a separate test vault name per connection row', async () => {
    const conn = {
      baseUrl: 'https://vault.internal:8774',
      clientId: 'rocketapi',
      verifySsl: true,
      allowInsecureHttp: false,
    };
    vi.mocked(tauriApi.listSecretManagerConnections).mockResolvedValue([
      { ...conn, id: 'conn-1', label: 'Prod' },
      { ...conn, id: 'conn-2', label: 'Staging' },
    ]);
    vi.mocked(tauriApi.testSecretManagerConnection).mockResolvedValue(undefined);
    renderDialog();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText(/vault name to test staging/i), 'stage-vault');

    expect(screen.getByLabelText(/vault name to test prod/i)).toHaveValue('');
    const testButtons = screen.getAllByRole('button', { name: /^test$/i });
    await user.click(testButtons[1]);
    expect(tauriApi.testSecretManagerConnection).toHaveBeenCalledWith('conn-2', 'stage-vault');
  });
});
