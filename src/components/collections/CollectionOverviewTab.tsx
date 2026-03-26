import { useState, useEffect, useCallback } from 'react';
import { Folder as FolderIcon, Save } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  getCollection,
  saveCollectionSettings,
  type Collection,
  type CollectionItem,
  type CollectionVariable,
} from '@/lib/tauri-api';
import { MethodBreakdown } from './MethodBreakdown';
import { RequestList } from './RequestList';
import { AuthEditor } from '@/components/request/AuthEditor';
import { HeadersEditor } from '@/components/request/HeadersEditor';
import { CollectionVariablesEditor } from './CollectionVariablesEditor';
import { usePaneStore } from '@/stores/pane-store';
import type { Auth } from '@/lib/tauri-api';
import type { AuthState, KeyValueEntry, CollectionTab, CollectionSection } from '@/types/pane-types';

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

  // Rust uses kebab-case "o-auth2", frontend uses "oauth2".
  if (authType === 'o-auth2' || authType === 'oauth2') {
    return {
      authType: 'oauth2',
      oauth2: {
        grantType: ((a.grantType as string) ?? 'client_credentials') as 'client_credentials' | 'password' | 'authorization_code' | 'implicit',
        authorizationUrl: (a.authorizationUrl as string) ?? '',
        tokenUrl: (a.tokenUrl as string) ?? '',
        callbackUrl: (a.callbackUrl as string) ?? 'https://exchange4all.local/webapp/#oidc-callback',
        clientId: (a.clientId as string) ?? '',
        clientSecret: (a.clientSecret as string) ?? '',
        scope: (a.scope as string) ?? '',
        state: (a.state as string) ?? '',
        username: (a.username as string) ?? '',
        password: (a.password as string) ?? '',
        clientAuthentication: ((a.clientAuthentication as string) ?? 'body') as 'header' | 'body',
        headerPrefix: (a.headerPrefix as string) ?? 'Bearer',
        addTokenTo: ((a.addTokenTo as string) ?? 'header') as 'header' | 'queryParams',
        verifySsl: (a.verifySsl as boolean) ?? true,
        accessToken: (a.accessToken as string) ?? '',
        refreshToken: (a.refreshToken as string) ?? '',
        expiresIn: (a.expiresIn as number) ?? null,
        tokenAcquiredAt: (a.tokenAcquiredAt as number) ?? null,
      },
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
    return { authType: 'basic', basic: { username: (a.username as string) ?? '', password: (a.password as string) ?? '' } };
  }

  if (authType === 'bearer') {
    return { authType: 'bearer', bearer: { token: (a.token as string) ?? '' } };
  }

  if (authType === 'api-key') {
    return { authType: 'api-key', apiKey: { key: (a.key as string) ?? '', value: (a.value as string) ?? '', addTo: ((a.addTo as string) ?? 'header') as 'header' | 'query' } };
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
      return { authType: 'basic', username: auth.basic?.username ?? '', password: auth.basic?.password ?? '' };
    case 'bearer':
      return { authType: 'bearer', token: auth.bearer?.token ?? '' };
    case 'api-key':
      return { authType: 'api-key', key: auth.apiKey?.key ?? '', value: auth.apiKey?.value ?? '', addTo: auth.apiKey?.addTo ?? 'header' };
    case 'oauth2': {
      const o = auth.oauth2;
      return {
        authType: 'o-auth2',
        grantType: o?.grantType ?? 'client_credentials',
        clientId: o?.clientId ?? '',
        clientSecret: o?.clientSecret ?? '',
        tokenUrl: o?.tokenUrl ?? '',
        scope: o?.scope || undefined,
        accessToken: o?.accessToken || undefined,
        refreshToken: o?.refreshToken || undefined,
        expiresAt: undefined,
      } as unknown as Auth;
    }
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

const TABS: { label: string; value: CollectionSection }[] = [
  { label: 'Overview', value: 'overview' },
  { label: 'Authorization', value: 'auth' },
  { label: 'Variables', value: 'variables' },
];

export function CollectionOverviewTab({ tab }: CollectionOverviewTabProps) {
  const collectionName = tab.collectionName;
  const updateCollectionSection = usePaneStore((s) => s.updateCollectionSection);

  const [collection, setCollection] = useState<Collection | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Editable settings state.
  const [description, setDescription] = useState('');
  const [auth, setAuth] = useState<AuthState>({ authType: 'none' });
  const [headers, setHeaders] = useState<KeyValueEntry[]>([]);
  const [variables, setVariables] = useState<CollectionVariable[]>([]);

  // Guard against stale section values from before the tab redesign.
  const validSections: CollectionSection[] = ['overview', 'auth', 'variables'];
  const activeSection = validSections.includes(tab.activeSection as CollectionSection)
    ? tab.activeSection!
    : 'overview';

  const handleSectionChange = useCallback((section: CollectionSection) => {
    updateCollectionSection(tab.id, section);
  }, [tab.id, updateCollectionSection]);

  // Load the collection on mount (settings are included in the response).
  useEffect(() => {
    setLoading(true);
    setError(null);
    getCollection(collectionName)
      .then((col) => {
        setCollection(col);
        const s = col.settings;
        setDescription(s.description ?? '');
        setAuth(toAuthState(s.auth));
        setHeaders(toKeyValueEntries(s.headers));
        setVariables(s.variables ?? []);
      })
      .catch((err) => {
        console.error('[CollectionOverviewTab] load failed', err);
        setError('Failed to load collection.');
      })
      .finally(() => setLoading(false));
  }, [collectionName]);

  // Save settings to disk (explicit save only, no auto-save).
  const saveSettings = useCallback(async () => {
    try {
      await saveCollectionSettings(collectionName, {
        auth: authStateToApi(auth),
        headers: headers.filter((h) => h.key).map((h) => ({
          key: h.key,
          value: h.value,
          enabled: h.enabled,
        })),
        description: description || undefined,
        variables,
      } as any);
    } catch (err) {
      console.error('[CollectionOverviewTab] save failed', err);
    }
  }, [collectionName, auth, headers, description, variables]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (error || !collection) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-destructive">
        {error ?? 'Collection not found.'}
      </div>
    );
  }

  const items = collection.root.items;
  const reqCount = countRequests(items);
  const folderCount = countFolders(items);
  const statsLine = `${plural(reqCount, 'request', 'requests')} · ${plural(folderCount, 'folder', 'folders')}`;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Collection header. */}
      <div className="shrink-0 border-b border-border/70 px-6 pt-4 pb-0">
        <div className="flex items-center gap-2 mb-1">
          <FolderIcon className="h-5 w-5 text-muted-foreground shrink-0" />
          <h1 className="text-lg font-semibold leading-tight truncate">
            {collection.name}
          </h1>
        </div>
        <p className="text-xs text-muted-foreground pl-7 mb-3">{statsLine}</p>

        {/* Tab bar. */}
        <div className="flex items-center gap-0">
          {TABS.map((t) => (
            <button
              key={t.value}
              onClick={() => handleSectionChange(t.value)}
              className={`h-8 px-4 text-xs font-medium transition-colors ${
                activeSection === t.value
                  ? 'border-b-2 border-primary text-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              {t.label}
              {t.value === 'variables' && variables.length > 0 && (
                <span className="ml-1 text-2xs text-muted-foreground">
                  ({variables.filter(v => v.enabled).length})
                </span>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* Tab content. */}
      <ScrollArea className="flex-1">
        <div className="p-6 max-w-3xl mx-auto space-y-6">

          {/* Overview tab. */}
          {activeSection === 'overview' && (
            <>
              {/* Description. */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground" htmlFor="col-description">
                  Description
                </label>
                <textarea
                  id="col-description"
                  rows={3}
                  placeholder="Add a description..."
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  onBlur={saveSettings}
                  className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm resize-none placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                />
              </div>

              {/* Method breakdown. */}
              <MethodBreakdown items={items} />

              {/* Default headers. */}
              <div className="space-y-2">
                <h3 className="text-xs font-medium text-muted-foreground">Default Headers</h3>
                <HeadersEditor headers={headers} onChange={setHeaders} />
                <div className="flex justify-end">
                  <Button size="sm" onClick={saveSettings} className="gap-1.5">
                    <Save className="h-3.5 w-3.5" />
                    Save
                  </Button>
                </div>
              </div>

              {/* Requests list. */}
              <div className="space-y-2">
                <h3 className="text-xs font-medium text-muted-foreground">Requests</h3>
                <RequestList items={items} collectionName={collectionName} />
              </div>
            </>
          )}

          {/* Authorization tab. */}
          {activeSection === 'auth' && (
            <div className="space-y-4">
              <div className="rounded-md border border-border bg-muted/30 px-3 py-2.5">
                <p className="text-xs text-muted-foreground">
                  This authorization method will be used for every request in this collection.
                  You can override this by specifying one in the request.
                </p>
              </div>

              <AuthEditor auth={auth} onChange={setAuth} />

              <div className="flex justify-end">
                <Button size="sm" onClick={saveSettings} className="gap-1.5">
                  <Save className="h-3.5 w-3.5" />
                  Save
                </Button>
              </div>
            </div>
          )}

          {/* Variables tab. */}
          {activeSection === 'variables' && (
            <div className="space-y-4">
              <CollectionVariablesEditor
                variables={variables}
                onChange={setVariables}
              />

              <div className="flex justify-end">
                <Button size="sm" onClick={saveSettings} className="gap-1.5">
                  <Save className="h-3.5 w-3.5" />
                  Save
                </Button>
              </div>
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
