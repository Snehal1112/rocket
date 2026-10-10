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
    expect(api.saveAgentConfig).toHaveBeenCalledWith({ ...claude, label: 'Claude Code' });
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
});
