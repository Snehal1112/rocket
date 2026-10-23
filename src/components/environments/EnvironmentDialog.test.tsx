// src/components/environments/EnvironmentDialog.test.tsx

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';
import type { ClientCertificate, Environment } from '@/lib/tauri-api';
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
    getDotEnvStatus: vi.fn().mockResolvedValue({
      baseDir: null,
      path: null,
      exists: false,
      keyCount: 0,
      error: null,
      blocked: false,
      usesDefault: true,
    }),
    getResolvedEnvironment: vi
      .fn()
      .mockResolvedValue({ name: '', variables: [], warnings: [] }),
    listVaultCertificates: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));

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

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op for test polyfill.
  };
}

describe('EnvironmentDialog inheritance and .env fields', () => {
  const base: Environment = { name: 'base', variables: [], externalSecrets: [] };

  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv, base]);
    vi.mocked(tauriApi.saveEnvironment).mockReset().mockResolvedValue(undefined);
  });

  async function saveLast() {
    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    return vi.mocked(tauriApi.saveEnvironment).mock.calls[0][1];
  }

  it('dialog_saves_the_chosen_parent_in_extends', async () => {
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.click(screen.getByRole('combobox', { name: 'Inherits from' }));
    await user.click(await screen.findByRole('option', { name: 'base' }));
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect((await saveLast()).extends).toBe('base');
  });

  it('dialog_saves_the_env_path_in_dotEnvFilePath', async () => {
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.type(screen.getByLabelText('.env file path'), '.env.prod');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect((await saveLast()).dotEnvFilePath).toBe('.env.prod');
  });

  it('dialog_clears_extends_when_no_parent_is_chosen', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([{ ...prodEnv, extends: 'base' }, base]);
    renderDialog();
    const user = userEvent.setup();
    await screen.findByLabelText('Variable key 1');

    await user.click(screen.getByRole('combobox', { name: 'Inherits from' }));
    await user.click(await screen.findByRole('option', { name: 'None' }));
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect((await saveLast()).extends).toBeUndefined();
  });

  it('dialog_keeps_a_deleted_parent_name_and_shows_a_warning', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([{ ...prodEnv, extends: 'gone' }]);
    renderDialog();
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    expect(screen.getByRole('combobox', { name: 'Inherits from' })).toHaveTextContent('gone');
    expect(screen.getByText(/parent "gone" was not found/i)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect((await saveLast()).extends).toBe('gone');
  });

  it('picker_is_disabled_when_there_are_no_candidates', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
    renderDialog();
    await screen.findByLabelText('Variable key 1');

    expect(screen.getByRole('combobox', { name: 'Inherits from' })).toBeDisabled();
  });
});

describe('EnvironmentDialog inherited rows', () => {
  const child: Environment = { ...prodEnv, name: 'child', extends: 'base' };
  const base: Environment = {
    name: 'base',
    variables: [{ key: 'TOKEN', value: 'abc', enabled: true, secret: false }],
    externalSecrets: [],
  };

  function resolved(variables: tauriApi.ResolvedVariable[], warnings: string[] = []) {
    vi.mocked(tauriApi.getResolvedEnvironment).mockResolvedValue({
      name: 'child',
      variables,
      warnings,
    });
  }

  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([child, base]);
    vi.mocked(tauriApi.saveEnvironment).mockReset().mockResolvedValue(undefined);
  });

  it('a_variable_overridden_by_the_child_is_not_listed_as_inherited', async () => {
    resolved([
      { key: 'HOST', value: 'https://api.example.com', enabled: true, secret: false, source: 'child' },
      { key: 'TOKEN', value: 'abc', enabled: true, secret: false, source: 'base' },
    ]);
    renderDialog();
    await screen.findByLabelText('Variable key 1');
    expect(await screen.findByLabelText('Inherited key 1')).toHaveValue('TOKEN');
    expect(screen.queryByLabelText('Inherited key 2')).not.toBeInTheDocument();
  });

  it('saving_does_not_include_inherited_rows', async () => {
    resolved([
      { key: 'HOST', value: 'https://api.example.com', enabled: true, secret: false, source: 'child' },
      { key: 'TOKEN', value: 'abc', enabled: true, secret: false, source: 'base' },
    ]);
    renderDialog();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    await screen.findByLabelText('Inherited key 1');
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
    const [, saved] = vi.mocked(tauriApi.saveEnvironment).mock.calls[0];
    expect(saved.variables.map((v) => v.key)).toEqual(['HOST2']);
  });

  it('a_missing_parent_shows_a_warning_and_save_still_works', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([{ ...child, extends: 'gone' }]);
    resolved([], ['Parent environment "gone" was not found', 'The .env file is missing']);
    renderDialog();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Variable key 1'), '2');
    expect(await screen.findByText(/the \.env file is missing/i)).toBeInTheDocument();
    // The missing parent is reported once, by the dialog's own warning.
    expect(screen.getAllByText(/"gone"/)).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());
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
});

describe('EnvironmentDialog empty states', () => {
  function renderWithCollection(collection: string | null) {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    useEnvStore.setState({ activeCollection: collection, activeEnvId: null });
    return render(
      <QueryClientProvider client={queryClient}>
        <EnvironmentDialog open onOpenChange={vi.fn()} />
      </QueryClientProvider>,
    );
  }

  it('asks for a collection instead of claiming there are no environments', async () => {
    renderWithCollection(null);
    expect(await screen.findByText('No collection selected')).toBeInTheDocument();
    expect(screen.queryByText('No environments yet')).not.toBeInTheDocument();
  });

  it('points at the + button when the collection has no environments', async () => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([]);
    renderWithCollection('my-collection');
    expect(await screen.findByText('No environments yet')).toBeInTheDocument();
    expect(screen.getByText('Use the + button in the list to create one.')).toBeInTheDocument();
    expect(screen.queryByText('No environment selected')).not.toBeInTheDocument();
  });

  it('does not say there are no environments while they are loading', async () => {
    vi.mocked(tauriApi.listEnvironments).mockReturnValue(new Promise(() => {}));
    renderWithCollection('my-collection');
    expect(await screen.findByText('Loading environments')).toBeInTheDocument();
    expect(screen.queryByText('No environments yet')).not.toBeInTheDocument();
  });
});

describe('EnvironmentDialog .env status line', () => {
  const status = (patch: Partial<tauriApi.DotEnvStatus>): tauriApi.DotEnvStatus => ({
    baseDir: '/work/col',
    path: '/work/col/.env',
    exists: true,
    keyCount: 4,
    error: null,
    blocked: false,
    usesDefault: false,
    ...patch,
  });

  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
  });

  async function renderWith(patch: Partial<tauriApi.DotEnvStatus>) {
    vi.mocked(tauriApi.getDotEnvStatus).mockResolvedValue(status(patch));
    renderDialog();
    await screen.findByLabelText('Variable key 1');
  }

  it('shows_found_with_the_key_count', async () => {
    await renderWith({});
    expect(await screen.findByText('Found .env, 4 keys')).toBeInTheDocument();
  });

  it('shows_not_found_for_a_missing_explicit_file', async () => {
    await renderWith({ exists: false, keyCount: 0 });
    expect(await screen.findByText(/^Not found/)).toBeInTheDocument();
  });

  it('shows_nothing_extra_for_a_missing_default_file', async () => {
    await renderWith({ exists: false, keyCount: 0, usesDefault: true });
    expect(await screen.findByText('No .env file in this folder')).toBeInTheDocument();
  });

  it('shows_not_read_when_blocked', async () => {
    await renderWith({ blocked: true, exists: false });
    expect(await screen.findByText(/host environment access/i)).toBeInTheDocument();
  });

  it('shows_the_error_text_in_amber', async () => {
    await renderWith({ error: 'Bad line 3' });
    const text = await screen.findByText(/Bad line 3/);
    expect(text.closest('p')).toHaveClass('text-amber-500');
  });

  it('refetches_the_status_after_a_save', async () => {
    vi.mocked(tauriApi.saveEnvironment).mockReset().mockResolvedValue(undefined);
    await renderWith({});
    await screen.findByText('Found .env, 4 keys');
    const before = vi.mocked(tauriApi.getDotEnvStatus).mock.calls.length;
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Variable key 1'), 'X');
    await user.click(screen.getByRole('button', { name: /^save$/i }));
    await vi.waitFor(() => expect(tauriApi.saveEnvironment).toHaveBeenCalled());

    await vi.waitFor(() =>
      expect(vi.mocked(tauriApi.getDotEnvStatus).mock.calls.length).toBeGreaterThan(before),
    );
  });
});
