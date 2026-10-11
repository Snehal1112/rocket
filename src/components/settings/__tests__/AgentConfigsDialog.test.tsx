import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AgentConfigsDialog } from '@/components/settings/AgentConfigsDialog';
import * as api from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  listAgentConfigs: vi.fn(),
  saveAgentConfig: vi.fn(),
  deleteAgentConfig: vi.fn(),
  testAgentConfig: vi.fn(),
  fetchExternalSecretNames: vi.fn(),
  listSecretManagerConnections: vi.fn(),
  setAgentCredential: vi.fn(),
  clearAgentCredential: vi.fn(),
  hasAgentCredential: vi.fn(),
}));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

// Radix Select calls pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}

const claude: api.AgentConfig = {
  id: 'agent-1',
  label: 'Claude',
  command: 'claude-agent-acp',
  args: ['--stdio', '--verbose'],
  workingDir: '/work',
  credentialEnvVar: 'ANTHROPIC_API_KEY',
  extraEnv: [],
  vaultConnectionId: 'conn-1',
  vaultName: 'prod-vault',
  vaultSecretId: 'sec-1',
  vaultSecretName: 'anthropic',
};

function renderDialog() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AgentConfigsDialog open onOpenChange={vi.fn()} />
    </QueryClientProvider>,
  );
}

describe('AgentConfigsDialog', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listAgentConfigs).mockResolvedValue([claude]);
    vi.mocked(api.saveAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.deleteAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.testAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.hasAgentCredential).mockResolvedValue(false);
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
      { id: 'conn-1', label: 'Vault one' } as api.SecretManagerConnection,
    ]);
  });

  it('lists the configured agents', async () => {
    renderDialog();
    expect(await screen.findByText('Claude')).toBeInTheDocument();
    expect(screen.getByText('claude-agent-acp')).toBeInTheDocument();
  });

  it('shows an empty state', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    renderDialog();
    expect(await screen.findByText('No agents configured.')).toBeInTheDocument();
  });

  it('edits a config and saves the trimmed values', async () => {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Edit agent' }));
    const label = await screen.findByLabelText('Label');
    expect(label).toHaveValue('Claude');
    expect(screen.getByLabelText(/Args/)).toHaveValue('--stdio, --verbose');
    await userEvent.clear(label);
    await userEvent.type(label, '  Claude Code  ');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(api.saveAgentConfig).toHaveBeenCalledWith({
      ...claude,
      credentialSource: 'vault',
      label: 'Claude Code',
    });
  });

  async function openEdit() {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
  }

  async function addVariable(name: string, value = '') {
    await userEvent.click(screen.getByRole('button', { name: 'Add variable' }));
    const names = screen.getAllByLabelText(/Variable name/);
    const values = screen.getAllByLabelText(/Variable value/);
    await userEvent.type(names[names.length - 1], name);
    if (value) await userEvent.type(values[values.length - 1], value);
  }

  it('saves added variables with the exact payload', async () => {
    await openEdit();
    await addVariable('ANTHROPIC_BASE_URL', 'https://proxy.example');
    await addVariable('HTTPS_PROXY', 'http://p:8080');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(api.saveAgentConfig).toHaveBeenCalledWith({
      ...claude,
      credentialSource: 'vault',
      extraEnv: [
        { name: 'ANTHROPIC_BASE_URL', value: 'https://proxy.example' },
        { name: 'HTTPS_PROXY', value: 'http://p:8080' },
      ],
    });
  });

  it('removes a variable row', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([
      {
        ...claude,
        extraEnv: [
          { name: 'A', value: '1' },
          { name: 'B', value: '2' },
        ],
      },
    ]);
    await openEdit();
    await userEvent.click(screen.getByRole('button', { name: 'Remove variable 1' }));
    expect(screen.getAllByLabelText(/Variable name/)).toHaveLength(1);
    expect(screen.getByLabelText('Variable name 1')).toHaveValue('B');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(api.saveAgentConfig).toHaveBeenCalledWith({
      ...claude,
      credentialSource: 'vault',
      extraEnv: [{ name: 'B', value: '2' }],
    });
  });

  it.each([
    ['1BAD', /do not start with a digit/],
    ['BAD NAME', /letters, digits and underscore/],
    ['A=B', /letters, digits and underscore/],
    ['CLAUDE_CONFIG_DIR', /reserved/],
    ['claude_config_dir', /reserved/],
    ['anthropic_api_key', /differ from the credential/],
    ['A'.repeat(129), /at most 128/],
  ])('flags %s and blocks Save', async (name, message) => {
    await openEdit();
    await addVariable(name);
    expect(screen.getByRole('alert')).toHaveTextContent(message);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(api.saveAgentConfig).not.toHaveBeenCalled();
  });

  it('flags an empty name and duplicates case-insensitively', async () => {
    await openEdit();
    await userEvent.click(screen.getByRole('button', { name: 'Add variable' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Name is required.');
    await userEvent.type(screen.getByLabelText('Variable name 1'), 'Foo');
    await addVariable('FOO');
    expect(screen.getByRole('alert')).toHaveTextContent('Duplicate variable name.');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('loads an old config without extraEnv as empty', async () => {
    const { extraEnv: _omit, ...old } = claude;
    vi.mocked(api.listAgentConfigs).mockResolvedValue([old as api.AgentConfig]);
    await openEdit();
    expect(screen.queryByLabelText(/Variable name/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(api.saveAgentConfig).toHaveBeenCalledWith({
      ...claude,
      credentialSource: 'vault',
      extraEnv: [],
    });
  });

  it('rejects an empty command', async () => {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await userEvent.clear(await screen.findByLabelText('Command'));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('command'));
    expect(api.saveAgentConfig).not.toHaveBeenCalled();
  });

  it('rejects an empty credential env var', async () => {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await userEvent.clear(await screen.findByLabelText('Credential Env Var'));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('credential env var'));
    expect(api.saveAgentConfig).not.toHaveBeenCalled();
  });

  describe('credential env var validation', () => {
    const fake = 'sk-test-not-a-real-key-123';

    async function setCredential(value: string) {
      const field = await screen.findByLabelText('Credential Env Var');
      await userEvent.clear(field);
      await userEvent.type(field, value);
    }

    it('saves a valid name', async () => {
      await openEdit();
      await setCredential('CLAUDE_CODE_OAUTH_TOKEN');
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      await userEvent.click(screen.getByRole('button', { name: 'Save' }));
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        credentialEnvVar: 'CLAUDE_CODE_OAUTH_TOKEN',
      });
    });

    it('flags a token-like value without echoing it and blocks Save', async () => {
      await openEdit();
      await setCredential(fake);
      const alert = screen.getByRole('alert');
      expect(alert).toHaveTextContent(/looks like a secret value/);
      expect(alert).toHaveTextContent(/treat it as exposed and revoke it/);
      expect(alert.textContent).not.toContain(fake);
      expect(document.body.textContent).not.toContain(fake);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('flags a name that starts with a digit', async () => {
      await openEdit();
      await setCredential('1KEY');
      expect(screen.getByRole('alert')).toHaveTextContent(/do not start with a digit/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('flags the reserved name', async () => {
      await openEdit();
      await setCredential('claude_config_dir');
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/);
    });

    it('shows the message on open for an invalid stored config and does not save', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, credentialEnvVar: fake }]);
      await openEdit();
      expect(screen.getByRole('alert')).toHaveTextContent(/looks like a secret value/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('explains in the helper text that the field takes a name', async () => {
      await openEdit();
      expect(screen.getByText(/The NAME of the environment variable/)).toHaveTextContent(
        /ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN/,
      );
    });

    it('maps the backend rule error to a friendly toast', async () => {
      vi.mocked(api.saveAgentConfig).mockRejectedValue(
        'Invalid input: agent config credential_env_var may contain only letters, digits and underscores',
      );
      await openEdit();
      await userEvent.click(screen.getByRole('button', { name: 'Save' }));
      await waitFor(() =>
        expect(toast.error).toHaveBeenCalledWith(
          'The credential variable name is not valid. Use only letters, digits and underscores.',
        ),
      );
    });
  });

  it('rejects a new agent with required fields missing', async () => {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Add Agent' }));
    await userEvent.type(await screen.findByLabelText('Label'), 'New');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(toast.error).toHaveBeenCalled();
    expect(api.saveAgentConfig).not.toHaveBeenCalled();
  });

  it('runs the test command and toasts success', async () => {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Test' }));
    await waitFor(() => expect(api.testAgentConfig).toHaveBeenCalledWith('agent-1'));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Agent test succeeded.'));
  });

  it('toasts the error when the test fails', async () => {
    vi.mocked(api.testAgentConfig).mockRejectedValue('spawn failed');
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Test' }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith('Agent test failed: spawn failed'),
    );
  });

  it('asks to confirm and then deletes', async () => {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Delete agent' }));
    expect(api.deleteAgentConfig).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(api.deleteAgentConfig).toHaveBeenCalledWith('agent-1'));
  });

  it('keeps the agent when the removal is cancelled', async () => {
    renderDialog();
    await userEvent.click(await screen.findByRole('button', { name: 'Delete agent' }));
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(api.deleteAgentConfig).not.toHaveBeenCalled();
    expect(screen.getByText('Claude')).toBeInTheDocument();
  });

  it('lets a user with no vault connection add an environment variable agent', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: /add agent/i }));
    await user.type(await screen.findByLabelText('Label'), 'Claude');
    await user.type(screen.getByLabelText('Command'), 'claude-agent-acp');
    await user.type(screen.getByLabelText('Credential Env Var'), 'CLAUDE_CODE_OAUTH_TOKEN');
    await user.click(screen.getByRole('radio', { name: /rocket environment variable/i }));
    await user.type(screen.getByLabelText('Environment variable name'), 'MY_CLAUDE_TOKEN');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
      credentialSource: 'env',
      sourceEnvVar: 'MY_CLAUDE_TOKEN',
      credentialEnvVar: 'CLAUDE_CODE_OAUTH_TOKEN',
      vaultConnectionId: '',
      vaultName: '',
      vaultSecretId: '',
    });
  });

  it('flags a token-looking environment variable name without repeating it', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: /add agent/i }));
    await user.type(await screen.findByLabelText('Label'), 'Claude');
    await user.type(screen.getByLabelText('Command'), 'claude-agent-acp');
    await user.type(screen.getByLabelText('Credential Env Var'), 'CLAUDE_CODE_OAUTH_TOKEN');
    await user.click(screen.getByRole('radio', { name: /rocket environment variable/i }));
    await user.type(screen.getByLabelText('Environment variable name'), 'sk-ant-oat01-FAKE');

    expect(await screen.findByText(/looks like a secret value/i)).toBeInTheDocument();
    expect(screen.getByText(/looks like a secret value/i).textContent).not.toContain('sk-ant');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(api.saveAgentConfig).not.toHaveBeenCalled();
  });

  it('keeps the vault fields and sends credentialSource vault for a vault agent', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    expect(await screen.findByLabelText('Vault Name')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /rocketvault secret/i })).toBeChecked();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
        credentialSource: 'vault',
        vaultConnectionId: 'conn-1',
      }),
    );
  });
});
