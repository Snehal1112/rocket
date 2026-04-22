import { BoxIcon, Check, Loader2, Save } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
import { TagsList } from '@/components/collections/TagsList';
import { AuthEditor } from '@/components/request/AuthEditor';
import { HeadersEditor } from '@/components/request/HeadersEditor';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useSaveButton } from '@/hooks/use-save-button';
import {
  type ApiOAuth2Auth,
  apiAuthToOAuth2State,
  oauth2StateToApiAuth,
} from '@/lib/oauth2-mapping';
import type { Auth } from '@/lib/tauri-api';
import {
  type Collection,
  type CollectionItem,
  type CollectionVariable,
  getCollection,
  saveCollectionSettings,
} from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { usePaneStore } from '@/stores/pane-store';
import type {
  AuthState,
  CollectionSection,
  CollectionTab,
  KeyValueEntry,
} from '@/types/pane-types';
import { CollectionVariablesEditor } from './CollectionVariablesEditor';
import { MethodBreakdown } from './MethodBreakdown';
import { RequestList } from './RequestList';

interface CollectionOverviewTabProps {
  tab: CollectionTab;
}

function countRequests(items: CollectionItem[]): number {
  let n = 0;
  for (const item of items) {
    if (item.type === 'request') n += 1;
    else n += countRequests(item.items);
  }
  return n;
}

function countFolders(items: CollectionItem[]): number {
  let n = 0;
  for (const item of items) {
    if (item.type === 'folder') {
      n += 1;
      n += countFolders(item.items);
    }
  }
  return n;
}

function plural(n: number, singular: string, pluralForm: string): string {
  return `${n} ${n === 1 ? singular : pluralForm}`;
}

// Convert flat Rust Auth (from API) to nested frontend AuthState.
function toAuthState(auth: Collection['settings']['auth']): AuthState {
  if (!auth) return { authType: 'none' };
  const a = auth as Record<string, unknown>;
  const authType = a.authType as string;

  // Rust uses "o-auth2" authType and OAuth2Flow shape: { flow, credentials, accessTokenUrl }.
  if (authType === 'o-auth2' || authType === 'oauth2') {
    return {
      authType: 'oauth2' as const,
      oauth2: apiAuthToOAuth2State(auth as unknown as ApiOAuth2Auth),
    };
  }

  if (authType === 'aws-sig-v4') {
    return {
      authType: 'aws-sig-v4',
      awsSigV4: {
        accessKey: (a.accessKey as string) ?? '',
        secretKey: (a.secretKey as string) ?? '',
        region: (a.region as string) ?? '',
        service: (a.service as string) ?? '',
        sessionToken: (a.sessionToken as string) ?? '',
      },
    };
  }

  if (authType === 'basic') {
    return {
      authType: 'basic',
      basic: {
        username: (a.username as string) ?? '',
        password: (a.password as string) ?? '',
      },
    };
  }

  if (authType === 'bearer') {
    return { authType: 'bearer', bearer: { token: (a.token as string) ?? '' } };
  }

  if (authType === 'api-key') {
    return {
      authType: 'api-key',
      apiKey: {
        key: (a.key as string) ?? '',
        value: (a.value as string) ?? '',
        addTo: ((a.placement as string) ?? 'header') as 'header' | 'query',
      },
    };
  }

  return { authType: 'none' };
}

// Convert nested frontend AuthState back to flat Rust Auth for persistence.
function authStateToApi(auth: AuthState): Auth | undefined {
  switch (auth.authType) {
    case 'none':
    case 'inherit':
      return undefined;
    case 'basic':
      return {
        authType: 'basic',
        username: auth.basic?.username ?? '',
        password: auth.basic?.password ?? '',
      };
    case 'bearer':
      return { authType: 'bearer', token: auth.bearer?.token ?? '' };
    case 'api-key':
      return {
        authType: 'api-key',
        key: auth.apiKey?.key ?? '',
        value: auth.apiKey?.value ?? '',
        placement: auth.apiKey?.addTo ?? 'header',
      };
    case 'oauth2':
      if (!auth.oauth2) return undefined;
      return oauth2StateToApiAuth(auth.oauth2) as Auth;
    case 'aws-sig-v4': {
      const a = auth.awsSigV4;
      return {
        authType: 'aws-sig-v4',
        accessKey: a?.accessKey ?? '',
        secretKey: a?.secretKey ?? '',
        region: a?.region ?? '',
        service: a?.service ?? '',
        sessionToken: a?.sessionToken || undefined,
      } as unknown as Auth;
    }
    default:
      return undefined;
  }
}

function toKeyValueEntries(
  headers: { key: string; value: string; enabled: boolean }[],
): KeyValueEntry[] {
  return headers.map((h, i) => ({
    id: String(i),
    key: h.key,
    value: h.value,
    enabled: h.enabled,
  }));
}

const COLLECTION_AUTH_TYPES: { label: string; value: AuthState['authType'] }[] = [
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
];

const TABS: { label: string; value: CollectionSection }[] = [
  { label: 'Overview', value: 'overview' },
  { label: 'Authorization', value: 'auth' },
  { label: 'Variables', value: 'variables' },
  { label: 'Readme', value: 'readme' },
  { label: 'Tags', value: 'tags' },
];

export function CollectionOverviewTab({ tab }: CollectionOverviewTabProps) {
  const collectionName = tab.collectionName;
  const updateCollectionSection = usePaneStore((s) => s.updateCollectionSection);
  const setCollectionAuth = useCollectionAuthStore((s) => s.setCollectionAuth);

  const [collection, setCollection] = useState<Collection | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Editable settings state.
  const [description, setDescription] = useState('');
  const [auth, setAuth] = useState<AuthState>({ authType: 'none' });
  const [headers, setHeaders] = useState<KeyValueEntry[]>([]);
  const [variables, setVariables] = useState<CollectionVariable[]>([]);
  const [readme, setReadme] = useState('');
  // True once the user edits any field; reset after successful save or reload.
  const [isDirty, setIsDirty] = useState(false);
  // Prevents the store-sync effect from firing with the empty initial auth
  // state before getCollection resolves, which would wipe a cached token.
  const [isLoaded, setIsLoaded] = useState(false);
  // Guard against stale section values from before the tab redesign.
  const validSections: CollectionSection[] = ['overview', 'auth', 'variables', 'readme', 'tags'];
  const activeSection = validSections.includes(tab.activeSection as CollectionSection)
    ? (tab.activeSection ?? 'overview')
    : 'overview';

  const handleSectionChange = useCallback(
    (section: CollectionSection) => {
      updateCollectionSection(tab.id, section);
    },
    [tab.id, updateCollectionSection],
  );

  // Load the collection on mount (settings are included in the response).
  useEffect(() => {
    setLoading(true);
    setIsLoaded(false);
    setError(null);
    getCollection(collectionName)
      .then((col) => {
        setCollection(col);
        const s = col.settings;
        setDescription(s.description ?? '');

        // Load auth from disk. For OAuth2 flows the access/refresh tokens are never
        // written to disk, so restore them from the in-memory store if available.
        const diskAuth = toAuthState(s.auth);
        const cachedAuth = useCollectionAuthStore.getState().getCollectionAuth(collectionName);
        if (
          diskAuth.authType === 'oauth2' &&
          diskAuth.oauth2 &&
          !diskAuth.oauth2.accessToken &&
          cachedAuth?.authType === 'oauth2' &&
          cachedAuth.oauth2?.accessToken
        ) {
          diskAuth.oauth2 = {
            ...diskAuth.oauth2,
            accessToken: cachedAuth.oauth2.accessToken,
            refreshToken: cachedAuth.oauth2.refreshToken ?? '',
            expiresIn: cachedAuth.oauth2.expiresIn ?? null,
            tokenAcquiredAt: cachedAuth.oauth2.tokenAcquiredAt ?? null,
            idToken: cachedAuth.oauth2.idToken ?? '',
            idTokenClaims: cachedAuth.oauth2.idTokenClaims ?? null,
            tokenType: cachedAuth.oauth2.tokenType ?? '',
            responseScope: cachedAuth.oauth2.responseScope ?? '',
          };
        }
        setAuth(diskAuth);

        setHeaders(toKeyValueEntries(s.headers));
        setVariables(s.variables ?? []);
        setReadme(s.readme ?? '');
        setIsDirty(false);
        setIsLoaded(true);
      })
      .catch((err) => {
        console.error('[CollectionOverviewTab] load failed', err);
        setError('Failed to load collection.');
      })
      .finally(() => setLoading(false));
  }, [collectionName]);

  // Keep the collection auth store in sync so execute-request.ts can resolve inherited auth.
  // Guarded by isLoaded to prevent the initial empty auth from wiping a cached token.
  useEffect(() => {
    if (!isLoaded) return;
    setCollectionAuth(collectionName, auth);
  }, [auth, collectionName, setCollectionAuth, isLoaded]);

  // Persist all settings to disk (no auto-save).
  const saveSettings = useCallback(async () => {
    await saveCollectionSettings(collectionName, {
      auth: authStateToApi(auth),
      headers: headers
        .filter((h) => h.key)
        .map((h) => ({
          key: h.key,
          value: h.value,
          enabled: h.enabled,
        })),
      description: description || undefined,
      readme: readme || undefined,
      variables,
    });
    setIsDirty(false);
  }, [collectionName, auth, headers, description, readme, variables]);

  const { state: saveState, trigger: triggerSave } = useSaveButton(
    saveSettings,
    'Failed to save settings',
  );

  const handleAuthTypeChange = useCallback((authType: AuthState['authType']) => {
    const next: AuthState = { authType };
    if (authType === 'basic') next.basic = { username: '', password: '' };
    if (authType === 'bearer') next.bearer = { token: '' };
    if (authType === 'api-key') next.apiKey = { key: '', value: '', addTo: 'header' };
    if (authType === 'oauth2')
      next.oauth2 = {
        grantType: 'client_credentials',
        authorizationUrl: '',
        tokenUrl: '',
        callbackUrl: 'https://exchange4all.local/webapp/#oidc-callback',
        clientId: '',
        clientSecret: '',
        scope: '',
        state: '',
        username: '',
        password: '',
        clientAuthentication: 'body',
        headerPrefix: 'Bearer',
        addTokenTo: 'header',
        verifySsl: true,
        accessToken: '',
        refreshToken: '',
        expiresIn: null,
        tokenAcquiredAt: null,
        usePkce: true,
        useSystemBrowser: false,
        tokenSource: 'accessToken',
        tokenId: '',
        refreshTokenUrl: '',
        autoFetchToken: true,
        autoRefreshToken: false,
        authParams: [],
        tokenParams: [],
        refreshParams: [],
        idToken: '',
        tokenType: '',
        responseScope: '',
        idTokenClaims: null,
      };
    if (authType === 'aws-sig-v4')
      next.awsSigV4 = { accessKey: '', secretKey: '', region: '', service: '', sessionToken: '' };
    setAuth(next);
    setIsDirty(true);
  }, []);

  if (loading) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        Loading...
      </div>
    );
  }

  if (error || !collection) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-destructive'>
        {error ?? 'Collection not found.'}
      </div>
    );
  }

  const items = collection.root.items;
  const reqCount = countRequests(items);
  const folderCount = countFolders(items);
  const statsLine = `${plural(reqCount, 'request', 'requests')} · ${plural(folderCount, 'folder', 'folders')}`;

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      {/* Collection header. */}
      <div className='shrink-0 border-b border-border/70 px-6 pt-4 pb-0'>
        <div className='flex items-center gap-2 mb-1'>
          <BoxIcon className='h-5 w-5 text-muted-foreground shrink-0' />
          <h1 className='text-lg font-semibold leading-tight truncate'>{collection.name}</h1>
        </div>
        <p className='text-xs text-muted-foreground pl-7 mb-3'>{statsLine}</p>

        {/* Tab bar. */}
        <div className='flex items-center gap-0'>
          {TABS.map((t) => (
            <button
              key={t.value}
              type='button'
              onClick={() => handleSectionChange(t.value)}
              className={`h-8 px-4 text-sm font-medium transition-colors ${
                activeSection === t.value
                  ? 'border-b-2 border-primary text-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              {t.label}
              {t.value === 'variables' && variables.length > 0 && (
                <span className='ml-1 text-2xs text-muted-foreground'>
                  ({variables.filter((v) => v.enabled).length})
                </span>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* Tab content. */}
      <ScrollArea className='flex-1'>
        <div className='p-6 max-w-3xl mx-auto space-y-6'>
          {/* Overview tab. */}
          {activeSection === 'overview' && (
            <>
              {/* Description. */}
              <div className='space-y-1.5'>
                <label
                  className='text-sm font-medium text-muted-foreground'
                  htmlFor='col-description'
                >
                  Description
                </label>
                <textarea
                  id='col-description'
                  rows={3}
                  placeholder='Add a description...'
                  value={description}
                  onChange={(e) => {
                    setDescription(e.target.value);
                    setIsDirty(true);
                  }}
                  onBlur={() => {
                    if (isDirty) void triggerSave();
                  }}
                  className='w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm resize-none placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring'
                />
              </div>

              {/* Method breakdown. */}
              <MethodBreakdown items={items} />

              {/* Default headers. */}
              <div className='space-y-2'>
                <h3 className='text-sm font-medium text-muted-foreground'>Default Headers</h3>
                <HeadersEditor
                  headers={headers}
                  onChange={(v) => {
                    setHeaders(v);
                    setIsDirty(true);
                  }}
                />
                <div className='flex justify-end'>
                  <Button
                    size='sm'
                    onClick={() => void triggerSave()}
                    disabled={!isDirty || saveState !== 'idle'}
                    className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
                  >
                    {saveState === 'saving' ? (
                      <Loader2 className='h-3.5 w-3.5 animate-spin' />
                    ) : saveState === 'success' ? (
                      <Check className='h-3.5 w-3.5' />
                    ) : (
                      <Save className='h-3.5 w-3.5' />
                    )}
                    {saveState === 'success' ? 'Saved' : 'Save'}
                  </Button>
                </div>
              </div>

              {/* Requests list. */}
              <div className='space-y-2'>
                <h3 className='text-sm font-medium text-muted-foreground'>Requests</h3>
                <RequestList items={items} collectionName={collectionName} />
              </div>
            </>
          )}

          {/* Authorization tab. */}
          {activeSection === 'auth' && (
            <div className='space-y-4'>
              <div className='rounded-md border border-border bg-muted/30 px-3 py-2.5'>
                <p className='text-xs text-muted-foreground'>
                  This authorization method will be used for every request in this collection. You
                  can override this by specifying one in the request.
                </p>
              </div>

              <div className='space-y-1.5'>
                <label
                  htmlFor='col-auth-type'
                  className='text-sm font-medium text-muted-foreground'
                >
                  Auth Type
                </label>
                <Select value={auth.authType} onValueChange={handleAuthTypeChange}>
                  <SelectTrigger id='col-auth-type' className='w-48 h-8 text-sm'>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {COLLECTION_AUTH_TYPES.map((t) => (
                      <SelectItem key={t.value} value={t.value}>
                        {t.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <AuthEditor
                auth={auth}
                onChange={(v) => {
                  setAuth(v);
                  setIsDirty(true);
                }}
              />

              <div className='flex justify-end'>
                <Button
                  size='sm'
                  onClick={() => void triggerSave()}
                  disabled={!isDirty || saveState !== 'idle'}
                  className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
                >
                  {saveState === 'saving' ? (
                    <Loader2 className='h-3.5 w-3.5 animate-spin' />
                  ) : saveState === 'success' ? (
                    <Check className='h-3.5 w-3.5' />
                  ) : (
                    <Save className='h-3.5 w-3.5' />
                  )}
                  {saveState === 'success' ? 'Saved' : 'Save'}
                </Button>
              </div>
            </div>
          )}

          {/* Variables tab. */}
          {activeSection === 'variables' && (
            <div className='space-y-4'>
              <CollectionVariablesEditor
                variables={variables}
                onChange={(v) => {
                  setVariables(v);
                  setIsDirty(true);
                }}
              />

              <div className='flex justify-end'>
                <Button
                  size='sm'
                  onClick={() => void triggerSave()}
                  disabled={!isDirty || saveState !== 'idle'}
                  className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
                >
                  {saveState === 'saving' ? (
                    <Loader2 className='h-3.5 w-3.5 animate-spin' />
                  ) : saveState === 'success' ? (
                    <Check className='h-3.5 w-3.5' />
                  ) : (
                    <Save className='h-3.5 w-3.5' />
                  )}
                  {saveState === 'success' ? 'Saved' : 'Save'}
                </Button>
              </div>
            </div>
          )}

          {/* Readme tab. */}
          {activeSection === 'readme' && (
            <div className='space-y-4'>
              <MarkdownEditor
                value={readme}
                onChange={(v) => {
                  setReadme(v);
                  setIsDirty(true);
                }}
                onBlur={() => {
                  if (isDirty) void triggerSave();
                }}
              />
              <div className='flex justify-end'>
                <Button
                  size='sm'
                  onClick={() => void triggerSave()}
                  disabled={!isDirty || saveState !== 'idle'}
                  className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
                >
                  {saveState === 'saving' ? (
                    <Loader2 className='h-3.5 w-3.5 animate-spin' />
                  ) : saveState === 'success' ? (
                    <Check className='h-3.5 w-3.5' />
                  ) : (
                    <Save className='h-3.5 w-3.5' />
                  )}
                  {saveState === 'success' ? 'Saved' : 'Save'}
                </Button>
              </div>
            </div>
          )}

          {/* Tags tab. */}
          {activeSection === 'tags' && (
            <div>
              <TagsList collection={collection} />
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
