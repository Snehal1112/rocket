import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { authStateForType } from '@/lib/auth-type-defaults';
import { stateToFolderAuth } from '@/lib/folder-settings-convert';
import type { FolderSettings } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

// Edits made in the folder Auth tab, followed through to what a request in the folder sends.

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
  getFolderChainVariables: vi.fn(),
  getRequestVariables: vi.fn(),
  getFolderSettings: vi.fn(),
}));
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  ...api,
}));
vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));
vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({ getQueryData: () => undefined }),
}));
vi.mock('@/hooks/useFolderVariableContext', () => ({
  useFolderVariableContext: () => ({ variableContext: new Map(), environmentName: undefined }),
}));
// The real editor fetches tokens over IPC, so it is replaced by buttons that patch the state.
vi.mock('@/components/request/oauth2/OAuth2AuthEditor', () => ({
  OAuth2AuthEditor: ({ patchOAuth2 }: { patchOAuth2: (p: Record<string, string>) => void }) => (
    <div>
      <button type='button' onClick={() => patchOAuth2({ accessToken: 'minted-for-a' })}>
        Fetch token
      </button>
      <button type='button' onClick={() => patchOAuth2({ clientId: 'client-b' })}>
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

import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import { AuthSection } from '../AuthSection';

const request: RequestState = {
  requestType: 'http',
  method: 'GET',
  url: 'https://api.example/ping',
  pathParams: [],
  queryParams: [],
  headers: [],
  body: { mode: 'none', content: '', formData: [] },
  auth: { authType: 'inherit' },
  settings: {
    verifySsl: true,
    followRedirects: true,
    maxRedirects: 5,
    timeoutMs: 0,
    encodeUrl: true,
  },
  docs: null,
  tags: [],
  assertions: [],
  actions: [],
};

const oauth = authStateForType('oauth2', { authType: 'none' });
const configA = stateToFolderAuth({
  ...oauth,
  oauth2: {
    ...(oauth.oauth2 as NonNullable<typeof oauth.oauth2>),
    tokenUrl: 'https://a.example/token',
    clientId: 'client-a',
  },
});

/** Renders the folder Auth tab and returns what the tab would save to folder.yml. */
function editFolderAuth(...buttons: string[]) {
  const onChange = vi.fn();
  const settings = { headers: [], auth: configA, variables: [] } as unknown as FolderSettings;
  render(
    <AuthSection collectionName='demo' folderPath='api' settings={settings} onChange={onChange} />,
  );
  for (const name of buttons) fireEvent.click(screen.getByRole('button', { name }));
  // The last saved shape, or the unchanged disk config when nothing was saved.
  return onChange.mock.calls.length ? onChange.mock.lastCall?.[0].auth : configA;
}

describe('folder Auth tab edits reaching the send path', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useFolderAuthStore.setState({ auths: {} });
    useCollectionAuthStore.setState({ auths: new Map() });
    api.getCollectionSettings.mockResolvedValue({ variables: [], headers: [], auth: null });
    api.getFolderChainVariables.mockResolvedValue([]);
    api.getRequestVariables.mockResolvedValue([]);
  });

  const send = async (saved: unknown) => {
    api.getFolderSettings.mockImplementation(async (_c: string, path: string) => ({
      auth: path === 'api' ? saved : null,
    }));
    return (await resolveRequestFieldsForPath('demo', 'api/get.yml', request)).auth;
  };

  it('sends the token fetched for the saved config', async () => {
    const saved = editFolderAuth('Fetch token');
    expect(await send(saved)).toEqual({ authType: 'bearer', token: 'minted-for-a' });
  });

  it.each([
    'Edit client id',
    'Edit token url',
    'Edit grant type',
  ])('never sends the old token after "%s" and a save', async (edit) => {
    const saved = editFolderAuth('Fetch token', edit);
    expect(useFolderAuthStore.getState().getFolderAuth('demo', 'api')?.oauth2?.accessToken).toBe(
      '',
    );
    expect(await send(saved)).toEqual({ authType: 'inherit' });
  });

  it('keeps sending the token after an edit outside the token config', async () => {
    const saved = editFolderAuth('Fetch token', 'Edit scope');
    expect(await send(saved)).toEqual({ authType: 'bearer', token: 'minted-for-a' });
  });
});
