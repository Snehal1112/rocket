import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import {
  EMPTY_CREDENTIAL_FORM,
  SCOPE_DEBOUNCE_MS,
  vaultNameForSave,
} from '../AgentCredentialSection';
import { CONNECTIONS, renderSection } from './section-harness';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  fetchExternalSecretNames: vi.fn(),
  listSecretManagerConnections: vi.fn(),
}));

const fetchMock = () => vi.mocked(api.fetchExternalSecretNames);

const SAVED_RV = {
  vaultConnectionId: 'rv',
  vaultName: 'prod',
  vaultSecretId: 's1',
  vaultSecretName: 'anthropic',
};

async function openConnectionList(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole('combobox', { name: 'Connection' }));
}

describe('vaultNameForSave', () => {
  it('returns the trimmed connection label for Azure', () => {
    const azure = CONNECTIONS[1];
    expect(vaultNameForSave(azure, 'ignored')).toBe('Team Azure');
  });

  it('returns the trimmed typed scope for other providers', () => {
    expect(vaultNameForSave(CONNECTIONS[0], '  prod  ')).toBe('prod');
  });

  it('returns the trimmed typed scope without a connection', () => {
    expect(vaultNameForSave(undefined, ' x ')).toBe('x');
  });
});

describe('AgentCredentialSection contract exports', () => {
  it('re-exports the form default and the debounce delay named in the index', () => {
    expect(EMPTY_CREDENTIAL_FORM.credentialSource).toBe('vault');
    expect(SCOPE_DEBOUNCE_MS).toBe(400);
  });
});

describe('AgentCredentialSection, secret manager source', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue(CONNECTIONS);
    fetchMock().mockResolvedValue([]);
  });

  it('names the source picker group Source and always shows its help', async () => {
    renderSection();
    expect(await screen.findByRole('group', { name: 'Source' })).toBeInTheDocument();
    expect(
      screen.getByText(/Where Rocket gets the key when an agent session starts/),
    ).toBeVisible();
    expect(screen.getByRole('radio', { name: 'Keychain' })).toBeInTheDocument();
  });

  it('lists every connection with its provider and disables unavailable ones', async () => {
    const user = userEvent.setup();
    renderSection();
    await openConnectionList(user);
    expect(screen.getByRole('option', { name: 'Prod vault (RocketVault)' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'Team Azure (Azure Key Vault)' })).toBeInTheDocument();
    const aws = screen.getByRole('option', { name: /Legacy AWS \(AWS Secrets Manager\)/ });
    expect(aws).toHaveAttribute('aria-disabled', 'true');
    expect(aws).toHaveTextContent('not available yet');
  });

  it('shows a scope field from the provider descriptor and loads after typing', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection();
    await openConnectionList(user);
    await user.click(screen.getByRole('option', { name: 'Prod vault (RocketVault)' }));
    expect(latest.current.vaultConnectionId).toBe('rv');
    const scope = await screen.findByLabelText('Vault Name');
    expect(scope).toHaveAttribute('placeholder', 'Vault name');
    expect(fetchMock()).not.toHaveBeenCalled();
    await user.type(scope, 'prod');
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledWith('rv', 'prod'));
    expect(fetchMock()).toHaveBeenCalledTimes(1);
  });

  it('makes no fetch while the scope is empty or only whitespace', async () => {
    const user = userEvent.setup();
    renderSection({ initial: { ...SAVED_RV, vaultName: '   ' } });
    await screen.findByLabelText('Vault Name');
    await new Promise((resolve) => setTimeout(resolve, SCOPE_DEBOUNCE_MS + 100));
    expect(fetchMock()).not.toHaveBeenCalled();
    await user.type(screen.getByLabelText('Vault Name'), 'p');
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledWith('rv', 'p'));
    expect(fetchMock()).toHaveBeenCalledTimes(1);
  });

  it('hides the scope for Azure, saves the label as the vault name and loads at once', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection();
    await openConnectionList(user);
    await user.click(screen.getByRole('option', { name: 'Team Azure (Azure Key Vault)' }));
    expect(latest.current.vaultName).toBe('Team Azure');
    expect(screen.queryByLabelText('Vault name')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Vault Name')).not.toBeInTheDocument();
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledWith('az', 'Team Azure'));
  });

  it('clears the chosen secret when the connection changes', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({ initial: SAVED_RV });
    await openConnectionList(user);
    await user.click(screen.getByRole('option', { name: 'Team Azure (Azure Key Vault)' }));
    expect(latest.current.vaultSecretId).toBe('');
    expect(latest.current.vaultSecretName).toBe('');
  });

  it('keeps the chosen secret while the scope changes', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({ initial: SAVED_RV });
    await user.type(await screen.findByLabelText('Vault Name'), 'x');
    expect(latest.current.vaultName).toBe('prodx');
    expect(latest.current.vaultSecretId).toBe('s1');
    expect(latest.current.vaultSecretName).toBe('anthropic');
  });

  it('auto-loads the saved vault and picks a secret from the list', async () => {
    const user = userEvent.setup();
    fetchMock().mockResolvedValue([
      { name: 'anthropic', secretId: 's1' },
      { name: 'other', secretId: 's2' },
    ]);
    const { latest } = renderSection({ initial: SAVED_RV });
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledWith('rv', 'prod'));
    await user.click(await screen.findByRole('combobox', { name: 'Secret' }));
    await user.click(await screen.findByRole('option', { name: 'other' }));
    expect(latest.current.vaultSecretId).toBe('s2');
    expect(latest.current.vaultSecretName).toBe('other');
  });

  it('refreshes the list from the icon button', async () => {
    const user = userEvent.setup();
    renderSection({ initial: SAVED_RV });
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledTimes(1));
    await user.click(await screen.findByRole('button', { name: 'Refresh secrets' }));
    await waitFor(() => expect(fetchMock()).toHaveBeenCalledTimes(2));
  });

  it('warns when the chosen secret is missing from the list and keeps it', async () => {
    fetchMock().mockResolvedValue([{ name: 'other', secretId: 's2' }]);
    const { latest } = renderSection({ initial: SAVED_RV });
    expect(await screen.findByText(/anthropic.*no longer in this list/)).toBeInTheDocument();
    expect(latest.current.vaultSecretId).toBe('s1');
  });

  it('shows a loading block while the secrets load', async () => {
    fetchMock().mockReturnValue(new Promise(() => undefined));
    renderSection({ initial: SAVED_RV });
    expect(await screen.findByText('Loading secrets…')).toBeInTheDocument();
  });

  it('shows an empty state when the vault has no secrets', async () => {
    renderSection({ initial: { ...SAVED_RV, vaultSecretId: '', vaultSecretName: '' } });
    expect(await screen.findByText(/No secrets found/)).toBeInTheDocument();
  });

  it('shows an error with Retry and recovers', async () => {
    const user = userEvent.setup();
    fetchMock().mockRejectedValueOnce('boom');
    fetchMock().mockResolvedValue([{ name: 'anthropic', secretId: 's1' }]);
    renderSection({ initial: SAVED_RV });
    expect(await screen.findByText(/boom/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(screen.queryByText(/boom/)).not.toBeInTheDocument());
    expect(fetchMock()).toHaveBeenCalledTimes(2);
  });

  it('shows an empty state when there are no connections', async () => {
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
    renderSection();
    expect(await screen.findByText(/No secret manager connections yet/)).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Environment' })).toBeInTheDocument();
  });

  it('shows an error with Retry when the connections fail to load', async () => {
    const user = userEvent.setup();
    vi.mocked(api.listSecretManagerConnections).mockRejectedValueOnce('down');
    renderSection();
    expect(await screen.findByText(/Could not load secret manager connections/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByRole('combobox', { name: 'Connection' })).toBeInTheDocument();
  });

  it('warns and does not fetch when the saved connection was deleted', async () => {
    renderSection({ initial: { ...SAVED_RV, vaultConnectionId: 'gone' } });
    expect(await screen.findByText(/saved connection no longer exists/)).toBeInTheDocument();
    expect(fetchMock()).not.toHaveBeenCalled();
  });

  it('warns and does not fetch when the saved connection has no fetcher', async () => {
    renderSection({ initial: { ...SAVED_RV, vaultConnectionId: 'aws1' } });
    expect(await screen.findByText(/AWS Secrets Manager is not available yet/)).toBeInTheDocument();
    expect(fetchMock()).not.toHaveBeenCalled();
  });

  it('shows the field errors it is given', async () => {
    renderSection({
      initial: SAVED_RV,
      errors: { vaultConnectionId: 'Pick a connection.', vaultSecretId: 'Pick a secret.' },
    });
    expect(await screen.findByText('Pick a connection.')).toBeInTheDocument();
    expect(await screen.findByText('Pick a secret.')).toBeInTheDocument();
  });
});
