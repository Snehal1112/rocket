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

  it('marks only the providers without an implementation as not available yet', async () => {
    Element.prototype.hasPointerCapture = () => false;
    Element.prototype.scrollIntoView = () => undefined;
    renderDialog();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /add connection/i }));
    await user.click(screen.getByRole('combobox', { name: /provider/i }));

    expect(await screen.findByRole('option', { name: /^azure key vault$/i })).toBeInTheDocument();
    expect(
      screen.getByRole('option', { name: /aws secrets manager \(not available yet\)/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('option', { name: /hashicorp vault \(not available yet\)/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('option', { name: /google secret manager \(not available yet\)/i }),
    ).toBeInTheDocument();
  });
});
