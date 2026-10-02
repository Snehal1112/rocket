// src/components/environments/EnvironmentDialog.test.tsx

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';
import type { Environment } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listEnvironments: vi.fn(),
    saveEnvironment: vi.fn(),
    deleteEnvironment: vi.fn(),
    getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
    getGlobalEnvironment: vi.fn().mockResolvedValue(null),
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
  };
});

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

const prodEnv: Environment = {
  name: 'prod',
  variables: [{ key: 'HOST', value: 'https://api.example.com', enabled: true, secret: false }],
  externalSecrets: [],
};

function renderDialog() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  useEnvStore.setState({ activeCollection: 'my-collection', activeEnvId: null });
  return render(
    <QueryClientProvider client={queryClient}>
      <EnvironmentDialog open onOpenChange={vi.fn()} />
    </QueryClientProvider>,
  );
}

describe('EnvironmentDialog tab switcher', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
  });

  it('defaults to the Variables tab and shows the variable table', async () => {
    renderDialog();
    expect(await screen.findByLabelText('Variable key 1')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /fetch secrets/i })).not.toBeInTheDocument();
  });

  it('switches to External Secrets and hides the variable table', async () => {
    renderDialog();
    await screen.findByLabelText('Variable key 1');
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: /external secrets/i }));

    expect(screen.queryByLabelText('Variable key 1')).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /add binding/i })).toBeInTheDocument();
  });

  it('switches back to Variables and shows the variable table again', async () => {
    renderDialog();
    await screen.findByLabelText('Variable key 1');
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: /external secrets/i }));
    await screen.findByRole('button', { name: /add binding/i });
    await user.click(screen.getByRole('tab', { name: /^variables$/i }));

    expect(await screen.findByLabelText('Variable key 1')).toBeInTheDocument();
  });
});

describe('EnvironmentDialog external secrets save flow', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
    vi.mocked(tauriApi.saveEnvironment).mockReset().mockResolvedValue(undefined);
  });

  it('saves an edited binding through the existing save mutation', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([
      {
        ...prodEnv,
        externalSecrets: [
          { alias: 'payments', connectionId: 'conn-1', vaultName: 'prod-vault', secretNames: [] },
        ],
      },
    ]);
    renderDialog();
    const user = userEvent.setup();

    await screen.findByLabelText('Variable key 1');
    await user.click(screen.getByRole('tab', { name: /external secrets/i }));
    await user.type(await screen.findByLabelText('Alias for binding 1'), '2');

    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, savedEnv] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(savedEnv.externalSecrets).toContainEqual(
      expect.objectContaining({
        alias: 'payments2',
        connectionId: 'conn-1',
        vaultName: 'prod-vault',
      }),
    );
  });

  it('does not save a binding that has no connection or vault name', async () => {
    renderDialog();
    const user = userEvent.setup();

    await screen.findByLabelText('Variable key 1');
    await user.click(screen.getByRole('tab', { name: /external secrets/i }));
    await user.click(await screen.findByRole('button', { name: /add binding/i }));
    await user.type(await screen.findByLabelText('Alias for binding 1'), 'payments');

    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(tauriApi.saveEnvironment).not.toHaveBeenCalled();
  });
});

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
    await user.type(
      screen.getByLabelText('Certificate file path for certificate 1'),
      'certs/a.pem',
    );
    await user.type(
      screen.getByLabelText('Private key file path for certificate 1'),
      'certs/a.key',
    );
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
