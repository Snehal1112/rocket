import { useState, useEffect, useCallback } from 'react';
import { Folder as FolderIcon, Save } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  getCollection,
  getCollectionSettings,
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
import { toApiAuth } from '@/lib/execute-request';
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

function toAuthState(auth: Collection['settings']['auth']): AuthState {
  if (!auth) return { authType: 'none' };
  return auth as unknown as AuthState;
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

  // Load the collection on mount.
  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      getCollection(collectionName),
      getCollectionSettings(collectionName),
    ])
      .then(([col, settings]) => {
        setCollection(col);
        setDescription(settings.description ?? '');
        setAuth(toAuthState(settings.auth));
        setHeaders(toKeyValueEntries(settings.headers));
        setVariables(settings.variables ?? []);
      })
      .catch((err) => {
        console.error('[CollectionOverviewTab] load failed', err);
        setError('Failed to load collection.');
      })
      .finally(() => setLoading(false));
  }, [collectionName]);

  // Save all settings to disk.
  const handleSave = useCallback(async () => {
    try {
      const apiAuth = toApiAuth(auth);
      await saveCollectionSettings(collectionName, {
        auth: apiAuth.authType !== 'none' ? apiAuth : undefined,
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

  // Save description on blur.
  const handleDescriptionBlur = useCallback(async () => {
    await handleSave();
  }, [handleSave]);

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
                <span className="ml-1 text-[10px] text-muted-foreground">
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
                  onBlur={handleDescriptionBlur}
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
                  <Button size="sm" onClick={handleSave} className="gap-1.5">
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
                <Button size="sm" onClick={handleSave} className="gap-1.5">
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
                <Button size="sm" onClick={handleSave} className="gap-1.5">
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
