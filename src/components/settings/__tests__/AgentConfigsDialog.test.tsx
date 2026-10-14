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

// Labels owned by the Plan 2 credential section. Align them with its test.
const L = {
  agentReadsAs: 'Agent reads it as',
  rocketReadsFrom: 'Rocket reads it from',
  keychainCredential: /keychain credential/i,
  // The pill text. It appears once, and the saved-state help text must not contain it.
  savedInKeychain: 'Saved in the keychain',
  savedHelp:
    'A credential is stored in the OS keychain on this computer. Use Replace to change it or Remove to delete it.',
  removeSaved: /^remove$/i,
  sourceSecretManager: /^secret manager$/i,
  sourceEnvironment: /^environment$/i,
  sourceKeychain: /^keychain$/i,
};

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

function renderDialog(onOpenChange: (open: boolean) => void = vi.fn()) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AgentConfigsDialog open onOpenChange={onOpenChange} />
    </QueryClientProvider>,
  );
}

type User = ReturnType<typeof userEvent.setup>;

async function openEdit(user: User = userEvent.setup()) {
  renderDialog();
  await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
  await screen.findByLabelText('Label');
  return user;
}

async function openAdd(user: User = userEvent.setup()) {
  renderDialog();
  await user.click(await screen.findByRole('button', { name: /add agent/i }));
  await screen.findByLabelText('Label');
  return user;
}

async function setField(user: User, label: string | RegExp, text: string) {
  const field = await screen.findByLabelText(label);
  await user.clear(field);
  await user.type(field, text);
}

const openAdvanced = (user: User) => user.click(screen.getByRole('button', { name: /^advanced/i }));
const clickSave = (user: User) => user.click(screen.getByRole('button', { name: 'Save' }));
const savedConfig = () => vi.mocked(api.saveAgentConfig).mock.calls[0][0];

async function addVariable(user: User, name: string, value = '') {
  await user.click(screen.getByRole('button', { name: 'Add variable' }));
  const names = screen.getAllByLabelText(/^Variable name \d+$/);
  const values = screen.getAllByLabelText(/^Variable value \d+$/);
  await user.type(names[names.length - 1], name);
  if (value) await user.type(values[values.length - 1], value);
}

describe('AgentConfigsDialog', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listAgentConfigs).mockResolvedValue([claude]);
    vi.mocked(api.saveAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.deleteAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.testAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.hasAgentCredential).mockResolvedValue(false);
    vi.mocked(api.fetchExternalSecretNames).mockResolvedValue([
      { secretId: 'sec-1', name: 'anthropic' },
    ]);
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
      { id: 'conn-1', label: 'Vault one' } as api.SecretManagerConnection,
    ]);
  });

  describe('list view', () => {
    it('lists the configured agents', async () => {
      renderDialog();
      expect(await screen.findByText('Claude')).toBeInTheDocument();
      expect(screen.getByText('claude-agent-acp')).toBeInTheDocument();
    });

    it('has a title and a description', async () => {
      renderDialog();
      expect(await screen.findByRole('dialog', { name: 'AI Agents' })).toBeInTheDocument();
      expect(screen.getByRole('dialog')).toHaveAccessibleDescription();
    });

    it('lays the dialog out as a column so the body scrolls and the footer is pinned', async () => {
      renderDialog();
      const dialog = await screen.findByRole('dialog', { name: 'AI Agents' });
      expect(dialog).toHaveClass('flex', 'max-h-[85vh]', 'flex-col');
    });

    it('shows an empty state', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      renderDialog();
      expect(await screen.findByText('No agents configured.')).toBeInTheDocument();
    });

    it('shows a retry when the agents cannot be loaded', async () => {
      vi.mocked(api.listAgentConfigs).mockRejectedValueOnce('boom');
      vi.mocked(api.listAgentConfigs).mockResolvedValue([claude]);
      const user = userEvent.setup();
      renderDialog();
      expect(await screen.findByText(/Failed to load agents/)).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Retry' }));
      expect(await screen.findByText('Claude')).toBeInTheDocument();
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
  });

  describe('edit form', () => {
    it('edits a config and saves the trimmed values, keeping the working directory', async () => {
      const user = await openEdit();
      expect(screen.getByLabelText('Label')).toHaveValue('Claude');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--stdio');
      expect(screen.getByLabelText('Argument 2')).toHaveValue('--verbose');
      await setField(user, 'Label', '  Claude Code  ');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        label: 'Claude Code',
      });
      expect(savedConfig().workingDir).toBe('/work');
    });

    it('does not render the working directory', async () => {
      await openEdit();
      expect(screen.queryByLabelText(/working director/i)).not.toBeInTheDocument();
    });

    it('keeps an argument that contains a comma as one argument', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, args: ['--flag=a,b'] }]);
      const user = await openEdit();
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--flag=a,b');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().args).toEqual(['--flag=a,b']);
    });

    it('saves added and removed arguments, one per row, dropping empty rows', async () => {
      const user = await openEdit();
      await user.click(screen.getByRole('button', { name: 'Add argument' }));
      await user.type(screen.getByLabelText('Argument 3'), '--extra');
      await user.click(screen.getByRole('button', { name: 'Add argument' }));
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().args).toEqual(['--stdio', '--verbose', '--extra']);
    });

    it('disables Save for an empty command without a toast', async () => {
      const user = await openEdit();
      await user.clear(screen.getByLabelText('Command'));
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(toast.error).not.toHaveBeenCalled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('shows no required help while Save is enabled', async () => {
      await openEdit();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
      expect(screen.queryByText(/^Required:/)).not.toBeInTheDocument();
    });

    it('lists only the missing fields under the footer while Save is disabled', async () => {
      const user = await openEdit();
      await user.clear(screen.getByLabelText('Command'));
      expect(screen.getByText('Required: command.')).toBeInTheDocument();
      await user.clear(screen.getByLabelText('Label'));
      expect(screen.getByText('Required: label and command.')).toBeInTheDocument();
      await user.clear(screen.getByLabelText(L.agentReadsAs));
      expect(screen.getByText('Required: label, command and key name.')).toBeInTheDocument();
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await user.type(screen.getByLabelText('Command'), 'claude-agent-acp');
      await user.type(screen.getByLabelText(L.agentReadsAs), 'ANTHROPIC_API_KEY');
      expect(screen.queryByText(/^Required:/)).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    });

    it('points the disabled Save at the help line', async () => {
      const user = await openEdit();
      await user.clear(await screen.findByLabelText('Command'));
      const save = screen.getByRole('button', { name: 'Save' });
      expect(save).toBeDisabled();
      const helpId = save.getAttribute('aria-describedby');
      expect(helpId).toBeTruthy();
      expect(document.getElementById(helpId ?? '')).toHaveTextContent(/command/i);
    });

    it('has no description on Save while it is enabled', async () => {
      await openEdit();
      expect(screen.getByRole('button', { name: 'Save' })).not.toHaveAttribute('aria-describedby');
    });

    it('lists the missing credential parts of a new agent', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      expect(screen.getByText('Required: label, connection and secret.')).toBeInTheDocument();
      await user.type(screen.getByLabelText('Label'), 'New');
      expect(screen.getByText('Required: connection and secret.')).toBeInTheDocument();
    });

    it('disables Save for an empty key name without a toast', async () => {
      const user = await openEdit();
      await user.clear(screen.getByLabelText(L.agentReadsAs));
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('disables Save for a new agent with required fields missing and shows no toast', async () => {
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'New');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(toast.error).not.toHaveBeenCalled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('puts Cancel before Save in the footer and returns to the list on Cancel', async () => {
      const user = await openEdit();
      const buttons = screen.getAllByRole('button').map((b) => b.textContent);
      expect(buttons.indexOf('Cancel')).toBeGreaterThan(-1);
      expect(buttons.indexOf('Cancel')).toBeLessThan(buttons.indexOf('Save'));
      await user.click(screen.getByRole('button', { name: 'Cancel' }));
      expect(await screen.findByText('claude-agent-acp')).toBeInTheDocument();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });
  });

  describe('while a save is in flight', () => {
    function deferSave() {
      let release: () => void = () => undefined;
      vi.mocked(api.saveAgentConfig).mockReturnValue(
        new Promise<void>((resolve) => {
          release = () => resolve();
        }),
      );
      return () => release();
    }

    it('disables Cancel until the save settles', async () => {
      const release = deferSave();
      const user = await openEdit();
      await clickSave(user);
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
      release();
      expect(await screen.findByText('claude-agent-acp')).toBeInTheDocument();
    });

    it('enables Cancel again when the save fails', async () => {
      vi.mocked(api.saveAgentConfig).mockRejectedValue(new Error('nope'));
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
    });

    it('does not let the dialog close mid-save', async () => {
      const release = deferSave();
      const onOpenChange = vi.fn();
      const user = userEvent.setup();
      renderDialog(onOpenChange);
      await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
      await screen.findByLabelText('Label');
      await clickSave(user);
      await user.keyboard('{Escape}');
      expect(onOpenChange).not.toHaveBeenCalled();
      release();
      await screen.findByText('claude-agent-acp');
      await user.click(screen.getByRole('button', { name: 'Edit agent' }));
      await user.keyboard('{Escape}');
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });
  });

  describe('advanced settings', () => {
    it('saves an idle timeout in seconds under tuning', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.type(screen.getByLabelText(/Idle timeout/), '300');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ tuning: { idleTimeoutSecs: 300 } });
    });

    it('sends no tuning when everything is left at the default', async () => {
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().tuning).toBeUndefined();
    });

    it('refuses an idle timeout outside 30 to 1800 seconds', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.type(screen.getByLabelText(/Idle timeout/), '5');
      expect(screen.getByRole('alert')).toHaveTextContent(/between 30 and 1800/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('shows a stored idle timeout', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, tuning: { idleTimeoutSecs: 600 } },
      ]);
      const user = await openEdit();
      expect(screen.getByRole('button', { name: /idle timeout 600 s/i })).toBeInTheDocument();
      await openAdvanced(user);
      expect(screen.getByLabelText(/Idle timeout/)).toHaveValue(600);
    });

    it('shows High as the reasoning level of an agent that has none stored', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.getByRole('radio', { name: /^high/i })).toBeChecked();
    });

    it('saves a lower reasoning level under tuning and omits the default', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('radio', { name: /^medium/i }));
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ tuning: { reasoning: 'medium' } });
    });

    it('shows a stored reasoning level', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, tuning: { reasoning: 'low' } },
      ]);
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.getByRole('radio', { name: /^low/i })).toBeChecked();
    });

    it('hides the reasoning for a generic agent but keeps its stored value on save', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, kind: 'generic', tuning: { reasoning: 'low' } },
      ]);
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.queryByRole('radio', { name: /^low/i })).not.toBeInTheDocument();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ kind: 'generic', tuning: { reasoning: 'low' } });
    });

    it('offers a model field for Gemini agents only and saves it under tuning', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'gemini' }]);
      const user = await openEdit();
      await openAdvanced(user);
      await user.type(screen.getByLabelText(/Gemini model/), 'gemini-2.5-pro');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ kind: 'gemini', tuning: { model: 'gemini-2.5-pro' } });
    });

    it('hides the model field for a Claude agent and refuses a Gemini model name with spaces', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.queryByLabelText(/Gemini model/)).not.toBeInTheDocument();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      await user.type(screen.getByLabelText(/Gemini model/), 'gemini 2.5');
      expect(screen.getByRole('alert')).toHaveTextContent(/letters, digits/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('drops a stored model for a non-Gemini agent on save', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, tuning: { model: 'gemini-2.5-pro' } },
      ]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().tuning).toBeUndefined();
    });
  });

  describe('extra environment variables', () => {
    it('saves added variables with the exact payload', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, 'ANTHROPIC_BASE_URL', 'https://proxy.example');
      await addVariable(user, 'HTTPS_PROXY', 'http://p:8080');
      await clickSave(user);
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
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('button', { name: 'Remove variable name 1' }));
      expect(screen.getAllByLabelText(/^Variable name \d+$/)).toHaveLength(1);
      expect(screen.getByLabelText('Variable name 1')).toHaveValue('B');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().extraEnv).toEqual([{ name: 'B', value: '2' }]);
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
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, name);
      expect(screen.getByRole('alert')).toHaveTextContent(message);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('shows no error for a freshly added blank row and drops it on save', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('button', { name: 'Add variable' }));
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().extraEnv).toEqual([]);
    });

    it('flags a row that has a value but no name, and duplicates case-insensitively', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('button', { name: 'Add variable' }));
      await user.type(screen.getByLabelText('Variable value 1'), 'x');
      expect(screen.getByRole('alert')).toHaveTextContent('Name is required.');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      await user.type(screen.getByLabelText('Variable name 1'), 'Foo');
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      await addVariable(user, 'FOO');
      expect(screen.getByRole('alert')).toHaveTextContent('Duplicate variable name.');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('loads an old config without extraEnv as empty', async () => {
      const { extraEnv: _omit, ...old } = claude;
      vi.mocked(api.listAgentConfigs).mockResolvedValue([old as api.AgentConfig]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        extraEnv: [],
      });
    });

    it.each([
      'GEMINI_CLI_HOME',
      'gemini_cli_trust_workspace',
      'GEMINI_CLI_IDE_WORKSPACE_PATH',
    ])('refuses the Gemini reserved variable %s for a Gemini agent', async (name) => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'gemini' }]);
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, name);
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('opens the closed Advanced section and disables Save when a kind switch makes a variable reserved', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, extraEnv: [{ name: 'GEMINI_CLI_HOME', value: 'x' }] },
      ]);
      const user = await openEdit();
      expect(screen.getByRole('button', { name: /^advanced/i })).toHaveAttribute(
        'aria-expanded',
        'false',
      );
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByRole('button', { name: /^advanced/i })).toHaveAttribute(
        'aria-expanded',
        'true',
      );
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('allows a Gemini reserved name as an extra variable for a Claude agent', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, 'GEMINI_CLI_HOME');
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    });
  });

  describe('key name validation', () => {
    const fake = 'sk-test-not-a-real-key-123';

    it('saves a valid name', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, 'CLAUDE_CODE_OAUTH_TOKEN');
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        credentialEnvVar: 'CLAUDE_CODE_OAUTH_TOKEN',
      });
    });

    it('flags a token-like value without echoing it and blocks Save', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, fake);
      const alert = screen.getByRole('alert');
      expect(alert).toHaveTextContent(/looks like a secret value/);
      expect(alert).toHaveTextContent(/treat it as exposed and revoke it/);
      expect(alert.textContent).not.toContain(fake);
      expect(alert.textContent).not.toContain('RocketVault');
      expect(document.body.textContent).not.toContain(fake);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('flags a name that starts with a digit', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, '1KEY');
      expect(screen.getByRole('alert')).toHaveTextContent(/do not start with a digit/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('flags the Claude reserved name', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, 'claude_config_dir');
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/);
    });

    it('flags a Gemini reserved key name for a Gemini agent only', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'gemini' }]);
      const user = await openEdit();
      await setField(user, L.agentReadsAs, 'GEMINI_CLI_HOME');
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      await user.click(screen.getByRole('radio', { name: /^claude code/i }));
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    });

    it('shows the message on open for an invalid stored config and does not save', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, credentialEnvVar: fake }]);
      await openEdit();
      expect(screen.getByRole('alert')).toHaveTextContent(/looks like a secret value/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('maps the backend rule error to a friendly toast', async () => {
      vi.mocked(api.saveAgentConfig).mockRejectedValue(
        'Invalid input: agent config credential_env_var may contain only letters, digits and underscores',
      );
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() =>
        expect(toast.error).toHaveBeenCalledWith(
          'The credential variable name is not valid. Use only letters, digits and underscores.',
        ),
      );
    });
  });

  describe('credential sources', () => {
    it('lets a user with no connection add an environment variable agent', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await setField(user, L.agentReadsAs, 'CLAUDE_CODE_OAUTH_TOKEN');
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'MY_CLAUDE_TOKEN');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({
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
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'sk-ant-oat01-FAKE');
      const message = await screen.findByText(/looks like a secret value/i);
      expect(message.textContent).not.toContain('sk-ant');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('keeps the secret manager fields and sends credentialSource vault for a vault agent', async () => {
      const user = await openEdit();
      expect(screen.getByRole('radio', { name: L.sourceSecretManager })).toBeChecked();
      await clickSave(user);
      await waitFor(() =>
        expect(savedConfig()).toMatchObject({
          credentialSource: 'vault',
          vaultConnectionId: 'conn-1',
          vaultName: 'prod-vault',
          vaultSecretId: 'sec-1',
          vaultSecretName: 'anthropic',
        }),
      );
    });

    it('saves the connection label as the vault name for an Azure agent with an arbitrary stored name', async () => {
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
        { id: 'conn-1', label: 'Prod KV', provider: 'azure' } as api.SecretManagerConnection,
      ]);
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, vaultName: 'whatever-old' }]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({
        credentialSource: 'vault',
        vaultConnectionId: 'conn-1',
        vaultName: 'Prod KV',
      });
    });

    it('keeps the typed vault name for a RocketVault connection', async () => {
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
        { id: 'conn-1', label: 'Vault one', provider: 'rocketvault' } as api.SecretManagerConnection,
      ]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().vaultName).toBe('prod-vault');
    });

    const FAKE = 'sk-ant-oat01-FAKE-VALUE-FOR-TESTS';

    async function openKeychainForm(user: User) {
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await setField(user, L.agentReadsAs, 'CLAUDE_CODE_OAUTH_TOKEN');
      await user.click(screen.getByRole('radio', { name: L.sourceKeychain }));
    }

    it('saves the config first and then the typed keychain credential', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.setAgentCredential).mockResolvedValue(undefined);
      const user = await openAdd();
      await openKeychainForm(user);
      await user.type(screen.getByLabelText(L.keychainCredential), FAKE);
      expect(screen.getByLabelText(L.keychainCredential)).toHaveAttribute('type', 'password');
      await clickSave(user);
      await waitFor(() => expect(api.setAgentCredential).toHaveBeenCalledTimes(1));
      expect(savedConfig().credentialSource).toBe('keychain');
      expect(vi.mocked(api.setAgentCredential).mock.calls[0]).toEqual([savedConfig().id, FAKE]);
      expect(vi.mocked(api.saveAgentConfig).mock.invocationCallOrder[0]).toBeLessThan(
        vi.mocked(api.setAgentCredential).mock.invocationCallOrder[0],
      );
      expect(JSON.stringify(vi.mocked(api.saveAgentConfig).mock.calls)).not.toContain(FAKE);
    });

    it('keeps the dialog open and hides the value when the credential cannot be saved', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.setAgentCredential).mockRejectedValue(
        new Error('the keychain could not be read'),
      );
      const user = await openAdd();
      await openKeychainForm(user);
      await user.type(screen.getByLabelText(L.keychainCredential), FAKE);
      await clickSave(user);
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
      expect(JSON.stringify(vi.mocked(toast.error).mock.calls)).not.toContain(FAKE);
      expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
      expect(screen.getByLabelText(L.keychainCredential)).toHaveValue('');
    });

    it('stays disabled while the keychain credential is still being written', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      let finish: () => void = () => undefined;
      vi.mocked(api.setAgentCredential).mockReturnValue(
        new Promise<void>((resolve) => {
          finish = () => resolve();
        }),
      );
      const user = await openAdd();
      await openKeychainForm(user);
      await user.type(screen.getByLabelText(L.keychainCredential), FAKE);
      await clickSave(user);
      await waitFor(() => expect(api.setAgentCredential).toHaveBeenCalledTimes(1));
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
      finish();
      expect(await screen.findByRole('button', { name: /add agent/i })).toBeInTheDocument();
    });

    it('blocks a new keychain agent with an empty credential', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await openKeychainForm(user);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('shows a saved credential as saved and keeps it on a blank field', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        {
          ...claude,
          credentialSource: 'keychain',
          vaultConnectionId: '',
          vaultName: '',
          vaultSecretId: '',
          vaultSecretName: '',
        },
      ]);
      vi.mocked(api.hasAgentCredential).mockResolvedValue(true);
      const user = await openEdit();
      expect(await screen.findByText(L.savedInKeychain)).toBeInTheDocument();
      expect(screen.getByText(L.savedHelp)).toBeInTheDocument();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.setAgentCredential).not.toHaveBeenCalled();
      expect(api.clearAgentCredential).not.toHaveBeenCalled();
    });

    it('replaces a saved keychain credential without clearing it first', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, credentialSource: 'keychain' },
      ]);
      vi.mocked(api.hasAgentCredential).mockResolvedValue(true);
      vi.mocked(api.setAgentCredential).mockResolvedValue(undefined);
      const user = await openEdit();
      await user.click(await screen.findByRole('button', { name: /^replace$/i }));
      await user.type(screen.getByPlaceholderText('Paste the token'), FAKE);
      await clickSave(user);
      await waitFor(() => expect(api.setAgentCredential).toHaveBeenCalledTimes(1));
      expect(vi.mocked(api.setAgentCredential).mock.calls[0]).toEqual(['agent-1', FAKE]);
      expect(api.clearAgentCredential).not.toHaveBeenCalled();
    });

    it('removes the saved credential when Remove is used and the agent is saved', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, credentialSource: 'keychain' },
      ]);
      vi.mocked(api.hasAgentCredential).mockResolvedValue(true);
      vi.mocked(api.clearAgentCredential).mockResolvedValue(undefined);
      const user = await openEdit();
      await user.click(await screen.findByRole('button', { name: L.removeSaved }));
      await clickSave(user);
      await waitFor(() => expect(api.clearAgentCredential).toHaveBeenCalledWith('agent-1'));
    });

    it('does not save a typed credential after the source is switched away from the keychain', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await openKeychainForm(user);
      await user.type(screen.getByLabelText(L.keychainCredential), FAKE);
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'MY_CLAUDE_TOKEN');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.setAgentCredential).not.toHaveBeenCalled();
    });
  });

  describe('save mapping and safety', () => {
    it('trims arguments and drops blank rows', async () => {
      const user = await openEdit();
      await user.click(screen.getByRole('button', { name: 'Add argument' }));
      await user.type(screen.getByLabelText('Argument 3'), '  --padded  ');
      await user.click(screen.getByRole('button', { name: 'Add argument' }));
      await user.type(screen.getByLabelText('Argument 4'), '   ');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().args).toEqual(['--stdio', '--verbose', '--padded']);
    });

    it('keeps the stored vault name when the connections have not loaded', async () => {
      vi.mocked(api.listSecretManagerConnections).mockReturnValue(new Promise(() => {}));
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, vaultName: 'whatever-old' }]);
      const user = await openEdit();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().vaultName).toBe('whatever-old');
    });

    it('keeps the stored vault name when the stored connection is missing from the list', async () => {
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
        { id: 'other', label: 'Other', provider: 'azure' } as api.SecretManagerConnection,
      ]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().vaultName).toBe('prod-vault');
    });

    it('hints at the highlighted fields when Save is disabled by an error only', async () => {
      const user = await openEdit();
      expect(screen.queryByText('Fix the highlighted fields.')).not.toBeInTheDocument();
      await setField(user, L.agentReadsAs, '1KEY');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(screen.getByText('Fix the highlighted fields.')).toBeInTheDocument();
      expect(screen.queryByText(/^Required:/)).not.toBeInTheDocument();
    });

    it('shows a generic error toast when the save fails', async () => {
      vi.mocked(api.saveAgentConfig).mockRejectedValue('disk full');
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(toast.error).toHaveBeenCalledWith('Could not save agent: disk full'));
      expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
    });

    it('closes the form after a successful save', async () => {
      const user = await openEdit();
      await clickSave(user);
      expect(await screen.findByText('claude-agent-acp')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
    });

    it('starts a new agent from a clean form after cancelling another one', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Leftover');
      await user.click(screen.getByRole('button', { name: 'Cancel' }));
      await user.click(await screen.findByRole('button', { name: /add agent/i }));
      expect(await screen.findByLabelText('Label')).toHaveValue('');
    });

    it('does not remount the form when a failed credential save turns a new agent into a stored one', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.setAgentCredential).mockRejectedValue(new Error('keychain locked'));
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await setField(user, L.agentReadsAs, 'CLAUDE_CODE_OAUTH_TOKEN');
      await user.click(screen.getByRole('radio', { name: L.sourceKeychain }));
      await user.type(screen.getByLabelText(L.keychainCredential), 'sk-ant-oat01-FAKE-VALUE');
      const before = screen.getByLabelText('Label');
      await clickSave(user);
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
      expect(screen.getByLabelText('Label')).toBe(before);
    });
  });

  describe('agent kind', () => {
    it('shows a stored Claude agent as Claude Code and saves no kind override', async () => {
      const user = await openEdit();
      expect(screen.getByRole('radio', { name: /^claude code/i })).toBeChecked();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().kind).toBeUndefined();
    });

    it('saves the chosen agent type', async () => {
      const user = await openEdit();
      await user.click(screen.getByRole('radio', { name: /^generic/i }));
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ kind: 'generic' });
    });

    it('opens a stored generic agent as generic', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'generic' }]);
      await openEdit();
      expect(screen.getByRole('radio', { name: /^generic/i })).toBeChecked();
    });

    it('prefills a new Gemini agent with the Gemini command, argument and key name', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByLabelText('Command')).toHaveValue('gemini');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--acp');
      expect(screen.getByLabelText(L.agentReadsAs)).toHaveValue('GEMINI_API_KEY');
    });

    it('keeps a command the user already typed when Gemini is chosen', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await setField(user, 'Command', '/opt/gemini');
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByLabelText('Command')).toHaveValue('/opt/gemini');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--acp');
    });

    it('does not change the fields of a stored agent when its type is switched', async () => {
      const user = await openEdit();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByLabelText('Command')).toHaveValue('claude-agent-acp');
      expect(screen.getByLabelText(L.agentReadsAs)).toHaveValue('ANTHROPIC_API_KEY');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--stdio');
    });

    it('saves a Gemini agent with kind gemini', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Gemini');
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'GEMINI_API_KEY');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({
        kind: 'gemini',
        command: 'gemini',
        args: ['--acp'],
        credentialEnvVar: 'GEMINI_API_KEY',
        credentialSource: 'env',
        sourceEnvVar: 'GEMINI_API_KEY',
      });
    });
  });
});
