import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import { stateToFolderAuth } from '@/lib/folder-settings-convert';
import type { FolderSettings } from '@/lib/tauri-api';
import { useFolderAuthStore } from '@/stores/folder-auth-store';

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
vi.mock('@/components/request/oauth2/OAuth2AuthEditor', () => ({
  OAuth2AuthEditor: ({
    oauth2,
    patchOAuth2,
  }: {
    oauth2: { accessToken: string };
    patchOAuth2: (patch: Record<string, string>) => void;
  }) => (
    <div>
      <div data-testid='oauth2-token'>{oauth2.accessToken}</div>
      <button type='button' onClick={() => patchOAuth2({ accessToken: 'fetched' })}>
        Fetch token
      </button>
      <button type='button' onClick={() => patchOAuth2({ clientId: 'edited' })}>
        Edit client id
      </button>
      <button type='button' onClick={() => patchOAuth2({ tokenUrl: 'https://b.example/token' })}>
        Edit token url
      </button>
      <button type='button' onClick={() => patchOAuth2({ grantType: 'password' })}>
        Edit grant type
      </button>
      <button type='button' onClick={() => patchOAuth2({ scope: 'read' })}>
        Edit scope
      </button>
    </div>
  ),
}));
vi.mock('@/hooks/useFolderVariableContext', () => ({
  useFolderVariableContext: () => ({ variableContext: new Map(), environmentName: undefined }),
}));
// Radix Select needs pointer APIs jsdom lacks, so it is replaced by a plain list of options.
vi.mock('@/components/ui/select', async () => {
  const React = await import('react');
  const Ctx = React.createContext<(v: string) => void>(() => undefined);
  return {
    Select: ({
      value,
      onValueChange,
      children,
    }: {
      value: string;
      onValueChange: (v: string) => void;
      children: React.ReactNode;
    }) => (
      <Ctx.Provider value={onValueChange}>
        <div data-testid='auth-type' data-value={value}>
          {children}
        </div>
      </Ctx.Provider>
    ),
    SelectTrigger: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
    SelectValue: () => null,
    SelectContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
    SelectItem: ({ value, children }: { value: string; children: React.ReactNode }) => {
      const pick = React.useContext(Ctx);
      return (
        <button type='button' onClick={() => pick(value)}>
          {children}
        </button>
      );
    },
  };
});

import { AuthSection } from '../AuthSection';

const settings = (auth: unknown = null): FolderSettings =>
  ({
    headers: [],
    auth,
    variables: [],
    preRequestScript: null,
    postResponseScript: null,
    testsScript: null,
    docs: null,
  }) as unknown as FolderSettings;

const renderSection = (auth: unknown = null, onChange = vi.fn()) =>
  render(
    <AuthSection
      collectionName='demo'
      folderPath='api'
      settings={settings(auth)}
      onChange={onChange}
    />,
  );

describe('AuthSection', () => {
  beforeEach(() => useFolderAuthStore.setState({ auths: {} }));

  it('shows Inherit and the explanatory note when the folder has no auth', () => {
    renderSection(null);
    expect(screen.getByTestId('auth-type')).toHaveAttribute('data-value', 'inherit');
    expect(screen.getByText(/cannot switch authorization off/)).toBeInTheDocument();
  });

  it('does not offer None for a folder', () => {
    renderSection(null);
    expect(screen.queryByRole('button', { name: 'None' })).toBeNull();
    expect(screen.queryByRole('button', { name: /No Auth/ })).toBeNull();
    expect(screen.getByRole('button', { name: 'Bearer' })).toBeInTheDocument();
  });

  it('keeps an on-disk none visible and does not rewrite it', () => {
    const onChange = vi.fn();
    renderSection({ authType: 'none' }, onChange);
    expect(screen.getByTestId('auth-type')).toHaveAttribute('data-value', 'none');
    expect(screen.getByRole('button', { name: 'No Auth (set on disk)' })).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it('loads an existing bearer token into the editor', () => {
    renderSection({ authType: 'bearer', token: 'abc' });
    expect(screen.getByLabelText('Bearer token')).toHaveValue('abc');
  });

  it('choosing a type sends the persisted auth with that type defaults', () => {
    const onChange = vi.fn();
    renderSection(null, onChange);
    fireEvent.click(screen.getByRole('button', { name: 'Bearer' }));
    expect(onChange).toHaveBeenLastCalledWith({ auth: { authType: 'bearer', token: '' } });
  });

  it('typing a token sends the persisted bearer auth', () => {
    const onChange = vi.fn();
    renderSection({ authType: 'bearer', token: 'a' }, onChange);
    fireEvent.change(screen.getByLabelText('Bearer token'), { target: { value: 'abc' } });
    expect(onChange).toHaveBeenLastCalledWith({ auth: { authType: 'bearer', token: 'abc' } });
  });

  it('choosing Inherit clears the folder auth', () => {
    const onChange = vi.fn();
    renderSection({ authType: 'bearer', token: 'a' }, onChange);
    fireEvent.click(screen.getByRole('button', { name: 'Inherit' }));
    expect(onChange).toHaveBeenLastCalledWith({ auth: undefined });
  });

  it('choosing Inherit removes the folder entry from the auth store', () => {
    renderSection(null);
    fireEvent.click(screen.getByRole('button', { name: 'Bearer' }));
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')?.authType).toBe('bearer');
    fireEvent.click(screen.getByRole('button', { name: 'Inherit' }));
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')).toBeUndefined();
  });

  it('a fetched OAuth2 token is cached without marking the folder changed', () => {
    const onChange = vi.fn();
    const state = authStateForType('oauth2', { authType: 'none' });
    renderSection(stateToFolderAuth(state), onChange);
    fireEvent.click(screen.getByRole('button', { name: 'Fetch token' }));
    expect(onChange).not.toHaveBeenCalled();
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')?.oauth2?.accessToken).toBe(
      'fetched',
    );
    expect(screen.getByTestId('oauth2-token')).toHaveTextContent('fetched');
  });

  it('an OAuth2 config edit still marks the folder changed', () => {
    const onChange = vi.fn();
    const state = authStateForType('oauth2', { authType: 'none' });
    renderSection(stateToFolderAuth(state), onChange);
    fireEvent.click(screen.getByRole('button', { name: 'Edit client id' }));
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it.each([
    'Edit client id',
    'Edit token url',
    'Edit grant type',
  ])('drops a fetched token after "%s"', (button) => {
    const state = authStateForType('oauth2', { authType: 'none' });
    renderSection(stateToFolderAuth(state));
    fireEvent.click(screen.getByRole('button', { name: 'Fetch token' }));
    fireEvent.click(screen.getByRole('button', { name: button }));
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')?.oauth2?.accessToken).toBe(
      '',
    );
    expect(screen.getByTestId('oauth2-token')).toHaveTextContent('');
    expect(screen.getByTestId('oauth2-token')).not.toHaveTextContent('fetched');
  });

  it('keeps a fetched token after an edit outside the token config', () => {
    const state = authStateForType('oauth2', { authType: 'none' });
    renderSection(stateToFolderAuth(state));
    fireEvent.click(screen.getByRole('button', { name: 'Fetch token' }));
    fireEvent.click(screen.getByRole('button', { name: 'Edit scope' }));
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')?.oauth2?.accessToken).toBe(
      'fetched',
    );
  });

  it('does not restore a cached token fetched for another OAuth2 config', () => {
    const state = authStateForType('oauth2', { authType: 'none' });
    const fields = state.oauth2 as NonNullable<typeof state.oauth2>;
    useFolderAuthStore.getState().setFolderAuth('demo', 'api', {
      ...state,
      oauth2: { ...fields, clientId: 'old-client', accessToken: 'stale' },
    });
    renderSection(stateToFolderAuth({ ...state, oauth2: { ...fields, clientId: 'new-client' } }));
    expect(screen.getByTestId('oauth2-token')).toHaveTextContent('');
    expect(screen.getByTestId('oauth2-token')).not.toHaveTextContent('stale');
  });

  it('restores a cached OAuth2 token into the editor', () => {
    const state = authStateForType('oauth2', { authType: 'none' });
    useFolderAuthStore.getState().setFolderAuth('demo', 'api', {
      ...state,
      oauth2: { ...(state.oauth2 as NonNullable<typeof state.oauth2>), accessToken: 'cached' },
    });
    renderSection(stateToFolderAuth(state));
    expect(screen.getByTestId('oauth2-token')).toHaveTextContent('cached');
  });

  it('writes every edit to the folder auth store', () => {
    renderSection(null);
    fireEvent.click(screen.getByRole('button', { name: 'OAuth 2.0' }));
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')?.authType).toBe('oauth2');
  });

  it('resets the editor when settings.auth changes from outside', () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <AuthSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ authType: 'bearer', token: 'a' })}
        onChange={onChange}
      />,
    );
    rerender(
      <AuthSection
        collectionName='demo'
        folderPath='api'
        settings={settings({ authType: 'bearer', token: 'z' })}
        onChange={onChange}
      />,
    );
    expect(screen.getByLabelText('Bearer token')).toHaveValue('z');
  });
});
