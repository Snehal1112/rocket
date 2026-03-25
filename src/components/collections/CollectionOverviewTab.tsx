import { useState, useEffect, useCallback } from 'react';
import { Folder as FolderIcon } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  getCollection,
  saveCollectionSettings,
  type Collection,
  type CollectionItem,
} from '@/lib/tauri-api';
import { MethodBreakdown } from './MethodBreakdown';
import { RequestList } from './RequestList';
import { AuthEditor } from '@/components/request/AuthEditor';
import { HeadersEditor } from '@/components/request/HeadersEditor';
import { usePaneStore } from '@/stores/pane-store';
import type { AuthState, KeyValueEntry, CollectionTab, CollectionSection } from '@/types/pane-types';

interface CollectionOverviewTabProps {
  tab: CollectionTab;
}

// Recursively counts requests (leaf nodes) across the entire tree.
function countRequests(items: CollectionItem[]): number {
  let n = 0;
  for (const item of items) {
    if (item.type === 'request') n += 1;
    else n += countRequests(item.items);
  }
  return n;
}

// Recursively counts folders (directory nodes) across the entire tree.
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

// Maps a CollectionSettings auth value to an AuthState shape for AuthEditor.
function toAuthState(auth: Collection['settings']['auth']): AuthState {
  if (!auth) return { authType: 'none' };
  // Auth from tauri-api and AuthState share the same discriminant structure
  // for none/basic/bearer/api-key.  Cast is safe for these shared variants.
  return auth as unknown as AuthState;
}

// Maps a collection Header array to KeyValueEntry[] for HeadersEditor.
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

export function CollectionOverviewTab({ tab }: CollectionOverviewTabProps) {
  const collectionName = tab.collectionName;
  const updateCollectionSection = usePaneStore((s) => s.updateCollectionSection);

  const [collection, setCollection] = useState<Collection | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Editable fields mirroring the loaded settings.
  const [description, setDescription] = useState('');
  const [auth, setAuth] = useState<AuthState>({ authType: 'none' });
  const [headers, setHeaders] = useState<KeyValueEntry[]>([]);

  const handleSectionChange = useCallback((section: string) => {
    updateCollectionSection(tab.id, section as CollectionSection);
  }, [tab.id, updateCollectionSection]);

  // Load the collection on mount.
  useEffect(() => {
    setLoading(true);
    setError(null);
    getCollection(collectionName)
      .then((col) => {
        setCollection(col);
        setDescription(col.settings.description ?? '');
        setAuth(toAuthState(col.settings.auth));
        setHeaders(toKeyValueEntries(col.settings.headers));
      })
      .catch((err) => {
        console.error('[CollectionOverviewTab] load failed', err);
        setError('Failed to load collection.');
      })
      .finally(() => setLoading(false));
  }, [collectionName]);

  // Save description on textarea blur.
  const handleDescriptionBlur = useCallback(async () => {
    try {
      await saveCollectionSettings(collectionName, {
        auth: auth.authType !== 'none' ? (auth as unknown as any) : undefined,
        headers: headers.filter((h) => h.key),
        description,
      } as any);
    } catch (err) {
      console.error('[CollectionOverviewTab] description save failed', err);
    }
  }, [collectionName, auth, headers, description]);

  // Save auth + headers together (called from the explicit Save button).
  const handleSaveSettings = useCallback(async () => {
    try {
      await saveCollectionSettings(collectionName, {
        auth: auth.authType !== 'none' ? (auth as unknown as any) : undefined,
        headers: headers.filter((h) => h.key),
        description,
      } as any);
    } catch (err) {
      console.error('[CollectionOverviewTab] settings save failed', err);
    }
  }, [collectionName, auth, headers, description]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        Loading…
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
    <ScrollArea className="h-full">
      <div className="p-6 space-y-6 max-w-3xl mx-auto">

        {/* Header: icon + name + stats. */}
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <FolderIcon className="h-5 w-5 text-muted-foreground shrink-0" />
            <h1 className="text-lg font-semibold leading-tight truncate">
              {collection.name}
            </h1>
          </div>
          <p className="text-xs text-muted-foreground pl-7">{statsLine}</p>
        </div>

        {/* Description textarea — saves on blur. */}
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground" htmlFor="col-description">
            Description
          </label>
          <textarea
            id="col-description"
            rows={3}
            placeholder="Add a description…"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            onBlur={handleDescriptionBlur}
            className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm resize-none placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          />
        </div>

        {/* Method breakdown. */}
        <MethodBreakdown items={items} />

        {/* Auth / Headers / Requests tabs inside a card. */}
        <Card>
          <CardContent className="pt-4">
            <Tabs value={tab.activeSection ?? 'requests'} onValueChange={handleSectionChange}>
              <TabsList className="w-full justify-start">
                <TabsTrigger value="auth">Auth</TabsTrigger>
                <TabsTrigger value="headers">Headers</TabsTrigger>
                <TabsTrigger value="requests">Requests</TabsTrigger>
              </TabsList>

              <TabsContent value="auth" className="mt-4 space-y-4">
                <AuthEditor auth={auth} onChange={setAuth} />
                <div className="flex justify-end">
                  <Button size="sm" onClick={handleSaveSettings}>Save</Button>
                </div>
              </TabsContent>

              <TabsContent value="headers" className="mt-4 space-y-4">
                <HeadersEditor headers={headers} onChange={setHeaders} />
                <div className="flex justify-end">
                  <Button size="sm" onClick={handleSaveSettings}>Save</Button>
                </div>
              </TabsContent>

              <TabsContent value="requests" className="mt-4">
                <RequestList items={items} collectionName={collectionName} />
              </TabsContent>
            </Tabs>
          </CardContent>
        </Card>

      </div>
    </ScrollArea>
  );
}
