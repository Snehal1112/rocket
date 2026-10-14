import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { credentialEnvPlaceholder, sourceEnvPlaceholder } from '../AgentCredentialSection';
import { CONNECTIONS, renderSection } from './section-harness';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  fetchExternalSecretNames: vi.fn(),
  listSecretManagerConnections: vi.fn(),
}));

describe('placeholders', () => {
  it.each([
    ['claude', 'ANTHROPIC_API_KEY', 'CLAUDE_CODE_OAUTH_TOKEN'],
    ['gemini', 'GEMINI_API_KEY', 'GEMINI_API_KEY'],
    ['generic', 'MY_AGENT_API_KEY', 'MY_AGENT_API_KEY'],
  ] as const)('uses kind specific examples for %s', (kind, agent, source) => {
    expect(credentialEnvPlaceholder(kind)).toBe(agent);
    expect(sourceEnvPlaceholder(kind)).toBe(source);
  });
});

describe('AgentCredentialSection, environment and keychain sources', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue(CONNECTIONS);
    vi.mocked(api.fetchExternalSecretNames).mockResolvedValue([]);
  });

  it('offers the three sources with the spec labels', async () => {
    renderSection();
    expect(await screen.findByRole('radio', { name: 'Secret manager' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Environment' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Keychain' })).toBeInTheDocument();
  });

  it('switches the source and clears the typed keychain value when leaving keychain', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({
      initial: { credentialSource: 'keychain', localCredential: 'tok', removeSavedCredential: true },
    });
    await user.click(screen.getByRole('radio', { name: 'Environment' }));
    expect(latest.current.credentialSource).toBe('env');
    expect(latest.current.localCredential).toBe('');
    expect(latest.current.removeSavedCredential).toBe(false);
  });

  it('prefills "Rocket reads it from" from "Agent reads it as" until it is edited', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({
      kind: 'gemini',
      initial: { credentialSource: 'env', credentialEnvVar: 'GEMINI_API_KEY' },
    });
    const from = screen.getByLabelText('Rocket reads it from');
    expect(from).toHaveValue('GEMINI_API_KEY');
    expect(latest.current.sourceEnvVar).toBe('GEMINI_API_KEY');

    const agent = screen.getByLabelText('Agent reads it as');
    await user.type(agent, '_2');
    expect(latest.current.sourceEnvVar).toBe('GEMINI_API_KEY_2');

    await user.clear(from);
    await user.type(from, 'MY_KEY');
    await user.type(agent, 'X');
    expect(latest.current.credentialEnvVar).toBe('GEMINI_API_KEY_2X');
    expect(latest.current.sourceEnvVar).toBe('MY_KEY');
  });

  it('never overwrites a saved source name that differs from the agent name', () => {
    const { latest } = renderSection({
      initial: {
        credentialSource: 'env',
        credentialEnvVar: 'ANTHROPIC_API_KEY',
        sourceEnvVar: 'MY_KEY',
      },
    });
    expect(latest.current.sourceEnvVar).toBe('MY_KEY');
    expect(screen.getByLabelText('Rocket reads it from')).toHaveValue('MY_KEY');
  });

  it.each(['vault', 'keychain'] as const)('never prefills the source name for the %s source', (credentialSource) => {
    const { latest } = renderSection({
      initial: { credentialSource, credentialEnvVar: 'ANTHROPIC_API_KEY', sourceEnvVar: '' },
    });
    expect(latest.current.sourceEnvVar).toBe('');
  });

  it('spells out the mapping in the help text and notes the shell requirement', () => {
    renderSection({
      kind: 'gemini',
      initial: {
        credentialSource: 'env',
        credentialEnvVar: 'GEMINI_API_KEY',
        sourceEnvVar: 'MY_KEY',
      },
    });
    expect(
      screen.getByText(/Rocket reads MY_KEY and gives it to the agent as GEMINI_API_KEY/),
    ).toBeInTheDocument();
    expect(screen.getByText(/exported in the shell Rocket was started from/)).toBeInTheDocument();
  });

  it('falls back to a general sentence while a name is empty', () => {
    renderSection({ initial: { credentialSource: 'env' } });
    expect(screen.queryByText(/gives it to the agent as/)).not.toBeInTheDocument();
    expect(screen.getByText(/NAME of the variable in Rocket's own environment/)).toBeInTheDocument();
  });

  it('shows the exported-in-the-shell note only for the environment source', async () => {
    renderSection({ initial: { credentialSource: 'keychain' } });
    expect(screen.queryByText(/exported in the shell/)).not.toBeInTheDocument();
  });

  it.each(['vault', 'env', 'keychain'] as const)(
    'shows "Agent reads it as" with its help once for the %s source',
    async (credentialSource) => {
      renderSection({ initial: { credentialSource } });
      expect(await screen.findByLabelText('Agent reads it as')).toBeInTheDocument();
      expect(screen.getByText(/not the key itself/)).toBeInTheDocument();
      expect(screen.getByText(/ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN/)).toBeInTheDocument();
      expect(screen.getAllByText(/short-lived token/)).toHaveLength(1);
    },
  );

  it.each([
    ['claude', 'ANTHROPIC_API_KEY'],
    ['gemini', 'GEMINI_API_KEY'],
    ['generic', 'MY_AGENT_API_KEY'],
  ] as const)('uses the %s placeholder for "Agent reads it as"', async (kind, placeholder) => {
    renderSection({ kind, initial: { credentialSource: 'keychain' } });
    expect(await screen.findByLabelText('Agent reads it as')).toHaveAttribute(
      'placeholder',
      placeholder,
    );
  });

  it('uses the kind specific placeholder for "Rocket reads it from"', () => {
    renderSection({ kind: 'claude', initial: { credentialSource: 'env', credentialEnvVar: '' } });
    expect(screen.getByLabelText('Rocket reads it from')).toHaveAttribute(
      'placeholder',
      'CLAUDE_CODE_OAUTH_TOKEN',
    );
  });

  it('shows the errors for both names', () => {
    renderSection({
      initial: { credentialSource: 'env' },
      errors: { credentialEnvVar: 'Bad agent name.', sourceEnvVar: 'Bad source name.' },
    });
    expect(screen.getByText('Bad agent name.')).toBeInTheDocument();
    expect(screen.getByText('Bad source name.')).toBeInTheDocument();
    expect(screen.getByLabelText('Agent reads it as')).toHaveAttribute('aria-invalid', 'true');
  });

  it('edits the keychain credential through the secret field', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({ initial: { credentialSource: 'keychain' } });
    await user.type(screen.getByLabelText('Keychain credential'), 'tok');
    expect(latest.current.localCredential).toBe('tok');
    expect(screen.getByText(/stored in the OS keychain/i)).toBeInTheDocument();
    expect(screen.queryByText('Saved in the keychain')).not.toBeInTheDocument();
  });

  it('explains a saved keychain credential and marks it for removal', async () => {
    const user = userEvent.setup();
    const { latest } = renderSection({
      initial: { credentialSource: 'keychain' },
      hasSavedCredential: true,
    });
    // The pill text is an exact match and appears once. The help uses different words.
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
    expect(
      screen.getByText(/A credential is stored in the OS keychain on this computer/i),
    ).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Keychain credential' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /remove/i }));
    expect(latest.current.removeSavedCredential).toBe(true);
    expect(latest.current.localCredential).toBe('');
    expect(screen.getByText(/never written to a file/i)).toBeInTheDocument();
  });

  it('shows the keychain error', () => {
    renderSection({
      initial: { credentialSource: 'keychain' },
      errors: { localCredential: 'Enter the credential to save.' },
    });
    expect(screen.getByText('Enter the credential to save.')).toBeInTheDocument();
  });
});
