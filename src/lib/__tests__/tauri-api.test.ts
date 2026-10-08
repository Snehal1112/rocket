import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  FOLDER_SETTINGS_SAVED_EVENT,
  type FolderSettings,
  getFolderSettings,
  isGitSshTrustFailure,
  parseGitNetworkError,
  saveFolderSettings,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('parseGitNetworkError', () => {
  it('parses an SSH unknown-host failure', () => {
    const parsed = parseGitNetworkError({
      code: 'sshUnknownHost',
      message:
        'Unknown SSH host git.example.com:22 (algorithm ssh-ed25519, fingerprint SHA256:abc)',
      host: 'git.example.com',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:abc',
    });

    expect(parsed.code).toBe('sshUnknownHost');
    expect(isGitSshTrustFailure(parsed)).toBe(true);
  });

  it('parses a TLS certificate failure without an algorithm field', () => {
    const parsed = parseGitNetworkError({
      code: 'tlsCertificateInvalid',
      message: 'Invalid TLS certificate for git.example.com:443 (fingerprint SHA256:invalid)',
      host: 'git.example.com',
      port: 443,
      fingerprint: 'SHA256:invalid',
    });

    expect(parsed).toEqual({
      code: 'tlsCertificateInvalid',
      message: 'Invalid TLS certificate for git.example.com:443 (fingerprint SHA256:invalid)',
      host: 'git.example.com',
      port: 443,
      fingerprint: 'SHA256:invalid',
    });
    expect(isGitSshTrustFailure(parsed)).toBe(false);
  });

  it('falls back to generic for an unrecognized shape', () => {
    expect(parseGitNetworkError(new Error('boom'))).toEqual({
      code: 'generic',
      message: 'Error: boom',
    });
  });

  it('falls back to generic when a TLS failure is missing required fields', () => {
    expect(
      parseGitNetworkError({
        code: 'tlsCertificateInvalid',
        message: 'Invalid TLS certificate',
        host: 'git.example.com',
        // port missing
      }),
    ).toEqual({
      code: 'generic',
      message: 'Invalid TLS certificate',
    });
  });
});

describe('listContracts in-flight dedup', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('coalesces two concurrent calls for the same collectionRoot into one invoke', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: 'c1' }]);
    const { listContracts } = await import('../tauri-api');

    const [a, b] = await Promise.all([
      listContracts('/ws/collections/my-api'),
      listContracts('/ws/collections/my-api'),
    ]);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(a).toEqual(b);
  });

  it('issues a fresh invoke for a different collectionRoot', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { listContracts } = await import('../tauri-api');

    await Promise.all([listContracts('/ws/collections/a'), listContracts('/ws/collections/b')]);

    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('clears the cached promise after a rejection, so the next call retries', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('boom'));
    const { listContracts } = await import('../tauri-api');

    await expect(listContracts('/ws/collections/my-api')).rejects.toThrow('boom');

    vi.mocked(invoke).mockResolvedValueOnce([{ id: 'c1' }]);
    const result = await listContracts('/ws/collections/my-api');

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(result).toEqual([{ id: 'c1' }]);
  });
});

describe('folder settings bindings', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('getFolderSettings calls get_folder_settings with collection and folderPath', async () => {
    const settings: FolderSettings = { headers: [], variables: [], docs: '# Auth' };
    vi.mocked(invoke).mockResolvedValueOnce(settings);

    await expect(getFolderSettings('my-api', 'auth/login')).resolves.toEqual(settings);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('get_folder_settings', {
      collection: 'my-api',
      folderPath: 'auth/login',
    });
  });

  it('saveFolderSettings sends the settings under the settings key', async () => {
    const settings: FolderSettings = {
      headers: [{ key: 'X-Team', value: 'core', enabled: true }],
      auth: { authType: 'bearer', token: '{{token}}' },
      variables: [],
      preRequestScript: "console.log('pre');",
      testsScript: "test('ok', () => {});",
    };
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await saveFolderSettings('my-api', 'auth', settings);

    expect(invoke).toHaveBeenCalledWith('save_folder_settings', {
      collection: 'my-api',
      folderPath: 'auth',
      settings,
    });
  });

  it('FOLDER_SETTINGS_SAVED_EVENT matches the Rust event tag', () => {
    // Must equal the tag asserted in folder_settings_saved_wire_shape (rocket-shared).
    expect(FOLDER_SETTINGS_SAVED_EVENT).toBe('folderSettingsSaved');
  });
});
