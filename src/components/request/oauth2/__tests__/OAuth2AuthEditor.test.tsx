import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';

const api = vi.hoisted(() => ({
  oauth2GetToken: vi.fn(),
  oauth2RefreshToken: vi.fn(),
  oauth2DecodeJwt: vi.fn(),
}));
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  ...api,
}));
// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({ value, 'aria-label': label }: { value: string; 'aria-label'?: string }) => (
    <input aria-label={label} value={value} readOnly />
  ),
}));

import { CLOSED_PROCESS_ENV } from '@/lib/process-env-gate';
import { useEnvStore } from '@/stores/env-store';
import { OAuth2AuthEditor, oauth2EditorScope } from '../OAuth2AuthEditor';

describe('oauth2_editor_scope_falls_back_to_collection_or_closed', () => {
  beforeEach(() => {
    useEnvStore.setState({ activeCollection: null, activeEnvId: null });
  });

  it('uses the collection when no scope prop is given', () => {
    expect(oauth2EditorScope(undefined, 'api', false)).toEqual({ kind: 'collection', name: 'api' });
  });

  it('is closed with no scope prop and no collection', () => {
    expect(oauth2EditorScope(undefined, undefined, false)).toEqual({ kind: 'closed' });
  });

  it('lets the scope prop win over the collection', () => {
    expect(oauth2EditorScope(CLOSED_PROCESS_ENV, 'api', true)).toEqual({ kind: 'closed' });
    expect(oauth2EditorScope(undefined, undefined, true)).toEqual({ kind: 'scratch' });
  });
});

const base = authStateForType('oauth2', { authType: 'none' }).oauth2 as NonNullable<
  ReturnType<typeof authStateForType>['oauth2']
>;
const configA = { ...base, tokenUrl: 'https://a.example/token', clientId: 'client-a' };

/** A token request that resolves only when the test says so. */
function pendingToken() {
  let resolve: (v: unknown) => void = () => undefined;
  api.oauth2GetToken.mockReturnValue(
    new Promise((r) => {
      resolve = r;
    }),
  );
  return () => resolve({ access_token: 'minted-for-a', token_type: 'Bearer' });
}

describe('OAuth2AuthEditor token fetch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.oauth2DecodeJwt.mockRejectedValue(new Error('opaque'));
  });

  it('applies a token fetched for the current config', async () => {
    const patch = vi.fn();
    const finish = pendingToken();
    render(<OAuth2AuthEditor oauth2={configA} patchOAuth2={patch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Get Access Token' }));
    await act(async () => finish());
    expect(patch).toHaveBeenCalledWith(expect.objectContaining({ accessToken: 'minted-for-a' }));
  });

  it('drops a token when the config changed while it was being fetched', async () => {
    const patch = vi.fn();
    const finish = pendingToken();
    const { rerender } = render(<OAuth2AuthEditor oauth2={configA} patchOAuth2={patch} />);
    fireEvent.click(screen.getByRole('button', { name: /Get Access Token/ }));
    rerender(
      <OAuth2AuthEditor oauth2={{ ...configA, clientId: 'client-b' }} patchOAuth2={patch} />,
    );
    await act(async () => finish());
    expect(patch).not.toHaveBeenCalledWith(
      expect.objectContaining({ accessToken: 'minted-for-a' }),
    );
    expect(screen.getByText(/settings changed while the token was being fetched/)).toBeTruthy();
  });
});
