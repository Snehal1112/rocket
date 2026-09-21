import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GitCredentialsDialog } from '@/components/git/GitCredentialsDialog';
import * as tauriApi from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import { createDeferred } from '@/test/deferred';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listSshKeyPaths: vi.fn().mockResolvedValue([]),
    getDefaultSshKeyPath: vi.fn().mockResolvedValue(null),
    loadGitCredentials: vi.fn().mockResolvedValue(null),
    saveGitCredentials: vi.fn(),
  };
});

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(tauriApi.listSshKeyPaths).mockResolvedValue([]);
  vi.mocked(tauriApi.getDefaultSshKeyPath).mockResolvedValue(null);
  vi.mocked(tauriApi.loadGitCredentials).mockResolvedValue(null);
});

function renderDialog() {
  const store = createGitStore();
  // Stub setCredentials. The real store action closes showCredentialsDialog
  // unconditionally as its first statement (see git-store.ts), which unmounts
  // this dialog's content before these tests could observe the saving/error UI.
  // Stubbing it isolates the Connect button's local `saving` state from that
  // unrelated, pre-existing store behavior while still letting us assert it
  // was invoked (preserving coverage of the "always activate credentials"
  // constraint). Note: because the real setCredentials closes the dialog
  // immediately in production, the keychain-error assertion below verifies
  // the component's own error-rendering logic in isolation, not something a
  // user can actually see today.
  const setCredentials = vi.fn();
  store.setState({ repositoryId: 'repo-1', showCredentialsDialog: true, setCredentials });
  render(
    <GitStoreProvider store={store}>
      <GitCredentialsDialog />
    </GitStoreProvider>,
  );
  return { store, setCredentials };
}

describe('GitCredentialsDialog saving state', () => {
  it('disables Connect and shows a busy state while credentials are being saved', async () => {
    const deferred = createDeferred<void>();
    vi.mocked(tauriApi.saveGitCredentials).mockReturnValue(deferred.promise);
    const { setCredentials } = renderDialog();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^connect$/i }));

    expect(screen.getByRole('button', { name: /connecting/i })).toBeDisabled();
    deferred.resolve();
    await vi.waitFor(() =>
      expect(screen.getByRole('button', { name: /^connect$/i })).not.toBeDisabled(),
    );
    expect(setCredentials).toHaveBeenCalled();
  });

  it('re-enables Connect and records the keychain error in store state after a failed save', async () => {
    vi.mocked(tauriApi.saveGitCredentials).mockRejectedValueOnce(new Error('keychain locked'));
    const { setCredentials } = renderDialog();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^connect$/i }));

    expect(await screen.findByText(/keychain locked/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^connect$/i })).not.toBeDisabled();
    // Critical constraint: setCredentials must still be called even when the
    // keychain save failed (existing "activate credentials anyway" behavior).
    expect(setCredentials).toHaveBeenCalled();
  });
});
