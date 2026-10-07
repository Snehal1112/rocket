import { describe, expect, it } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import {
  entriesToHeaders,
  folderAuthToState,
  folderAuthTypeOptions,
  folderChainPath,
  headersToEntries,
  restoreOAuth2Tokens,
  sameOAuth2Config,
  stateToFolderAuth,
} from '@/lib/folder-settings-convert';

describe('headersToEntries', () => {
  it('maps headers to editor rows with unique ids', () => {
    const rows = headersToEntries([
      { key: 'X-A', value: '1', enabled: true },
      { key: 'X-B', value: '2', enabled: false },
    ]);
    expect(rows.map((r) => [r.key, r.value, r.enabled])).toEqual([
      ['X-A', '1', true],
      ['X-B', '2', false],
    ]);
    expect(new Set(rows.map((r) => r.id)).size).toBe(2);
  });
});

describe('entriesToHeaders', () => {
  it('drops blank-key rows and keeps enabled false', () => {
    const out = entriesToHeaders(
      [
        { id: '0', key: 'X-A', value: '1', enabled: false },
        { id: '1', key: '', value: 'draft', enabled: true },
      ],
      [],
    );
    expect(out).toEqual([{ key: 'X-A', value: '1', enabled: false }]);
  });

  it('carries the description over by key', () => {
    const previous = [{ key: 'X-A', value: 'old', enabled: true, description: 'why' }];
    const out = entriesToHeaders([{ id: '0', key: 'X-A', value: 'new', enabled: true }], previous);
    expect(out).toEqual([{ key: 'X-A', value: 'new', enabled: true, description: 'why' }]);
  });

  it('adds no description key when the header had none', () => {
    const out = entriesToHeaders(
      [{ id: '0', key: 'X-A', value: '1', enabled: true }],
      [{ key: 'X-A', value: '1', enabled: true, description: null }],
    );
    expect('description' in out[0]).toBe(false);
  });
});

describe('folderChainPath', () => {
  it('is empty for the root folder', () => {
    expect(folderChainPath('')).toBe('');
  });

  it('is a synthetic file path inside the folder', () => {
    expect(folderChainPath('api/users')).toBe('api/users/folder.yml');
  });
});

describe('folder auth conversion', () => {
  it('maps no folder auth to Inherit and back to no auth', () => {
    expect(folderAuthToState(null)).toEqual({ authType: 'inherit' });
    expect(folderAuthToState(undefined)).toEqual({ authType: 'inherit' });
    expect(stateToFolderAuth({ authType: 'inherit' })).toBeUndefined();
  });

  it('keeps an on-disk none as none', () => {
    expect(folderAuthToState({ authType: 'none' })).toEqual({ authType: 'none' });
    expect(stateToFolderAuth({ authType: 'none' })).toEqual({ authType: 'none' });
  });

  it('round-trips bearer through the persisted shape', () => {
    const state = { authType: 'bearer', bearer: { token: 't' } } as const;
    const persisted = stateToFolderAuth(state);
    expect(persisted).toEqual({ authType: 'bearer', token: 't' });
    expect(folderAuthToState(persisted)).toEqual(state);
  });
});

describe('folderAuthTypeOptions', () => {
  it('does not offer None', () => {
    expect(folderAuthTypeOptions('basic').map((o) => o.value)).not.toContain('none');
    expect(folderAuthTypeOptions('inherit')[0]).toEqual({ label: 'Inherit', value: 'inherit' });
  });

  it('shows an existing on-disk none as a read-only first entry', () => {
    expect(folderAuthTypeOptions('none')[0]).toEqual({
      label: 'No Auth (set on disk)',
      value: 'none',
    });
  });
});

describe('restoreOAuth2Tokens', () => {
  const disk = authStateForType('oauth2', { authType: 'none' });
  const cached = {
    ...disk,
    oauth2: {
      ...(disk.oauth2 as NonNullable<typeof disk.oauth2>),
      accessToken: 'tok',
      refreshToken: 'ref',
    },
  };

  it('fills the tokens a disk copy lacks', () => {
    const out = restoreOAuth2Tokens(disk, cached);
    expect(out.oauth2?.accessToken).toBe('tok');
    expect(out.oauth2?.refreshToken).toBe('ref');
  });

  it('leaves a disk copy that already has a token alone', () => {
    const withToken = {
      ...disk,
      oauth2: { ...(disk.oauth2 as NonNullable<typeof disk.oauth2>), accessToken: 'mine' },
    };
    expect(restoreOAuth2Tokens(withToken, cached)).toBe(withToken);
  });

  it.each([
    ['grant type', { grantType: 'password' as const }],
    ['token url', { tokenUrl: 'https://other.example/token' }],
    ['client id', { clientId: 'other-client' }],
  ])('does not restore a token whose %s differs from the disk config', (_name, patch) => {
    const stale = { ...cached, oauth2: { ...cached.oauth2, ...patch } };
    expect(restoreOAuth2Tokens(disk, stale)).toBe(disk);
  });

  it('restores a token when only fields outside the fingerprint differ', () => {
    const other = { ...cached, oauth2: { ...cached.oauth2, scope: 'read' } };
    expect(restoreOAuth2Tokens(disk, other).oauth2?.accessToken).toBe('tok');
  });

  it('leaves non-OAuth2 state and a missing cache alone', () => {
    const bearer = { authType: 'bearer', bearer: { token: 't' } } as const;
    expect(restoreOAuth2Tokens(bearer, cached)).toBe(bearer);
    expect(restoreOAuth2Tokens(disk, undefined)).toBe(disk);
  });
});

describe('sameOAuth2Config', () => {
  const base = authStateForType('oauth2', { authType: 'none' });
  const fields = base.oauth2 as NonNullable<typeof base.oauth2>;
  const make = (patch: Partial<typeof fields>) => ({ ...base, oauth2: { ...fields, ...patch } });

  it('matches the same grant type, token url and client id', () => {
    expect(sameOAuth2Config(make({ scope: 'a' }), make({ scope: 'b' }))).toBe(true);
  });

  it('does not match when either side is not OAuth2', () => {
    expect(sameOAuth2Config(base, { authType: 'inherit' })).toBe(false);
    expect(sameOAuth2Config({ authType: 'bearer', bearer: { token: '' } }, base)).toBe(false);
  });

  it('compares the authorization url for the implicit grant', () => {
    const a = make({ grantType: 'implicit', authorizationUrl: 'https://a.example/auth' });
    const b = make({ grantType: 'implicit', authorizationUrl: 'https://b.example/auth' });
    expect(sameOAuth2Config(a, b)).toBe(false);
  });
});
