import { useEffect, useState, useCallback } from 'react';
import {
  listCollections,
  getCollection,
  onCollectionChanged,
  type CollectionSummary,
  type Collection,
  type CollectionItem,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { createDefaultRequest } from '@/lib/pane-utils';
import type { Tab, RequestState } from '@/types/pane-types';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { cn } from '@/lib/utils';
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  FileText,
  Search,
  Plus,
} from 'lucide-react';
import { HistoryPanel } from '@/components/history/HistoryPanel';

// Returns Tailwind text color for an HTTP method.
function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case 'GET':    return 'text-emerald-500';
    case 'POST':   return 'text-amber-500';
    case 'PUT':    return 'text-blue-500';
    case 'PATCH':  return 'text-violet-500';
    case 'DELETE': return 'text-red-500';
    case 'OPTIONS': return 'text-cyan-500';
    case 'HEAD':   return 'text-pink-500';
    default:       return 'text-muted-foreground';
  }
}

// Renders a single request item in the collection tree.
function RequestNode({
  name,
  method,
  collectionName,
  path,
}: {
  name: string;
  method: string;
  collectionName: string;
  path: string;
}) {
  function handleClick() {
    const tabId = `${collectionName}/${path}`;
    const request: RequestState = {
      ...createDefaultRequest(),
      method: method as RequestState['method'],
    };
    const tab: Tab = {
      id: tabId,
      title: name,
      tabType: 'request',
      request,
      response: null,
      isDirty: false,
      source: { collection: collectionName, path },
    };
    usePaneStore.getState().openTab(tab);
  }

  return (
    <button
      type="button"
      className="flex items-center gap-1.5 w-full px-2 py-1 text-left text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
      onClick={handleClick}
      aria-label={`Open ${method} ${name}`}
    >
      <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      <span className={cn('w-9 shrink-0 font-semibold text-[10px]', methodColor(method))}>
        {method}
      </span>
      <span className="truncate text-foreground">{name}</span>
    </button>
  );
}

// Renders a folder node with expandable children.
function FolderNode({
  name,
  items,
  collectionName,
  basePath,
  depth,
  filter,
}: {
  name: string;
  items: CollectionItem[];
  collectionName: string;
  basePath: string;
  depth: number;
  filter: string;
}) {
  const [expanded, setExpanded] = useState(depth < 2);

  // Auto-expand when a search filter is active.
  useEffect(() => {
    if (filter) setExpanded(true);
  }, [filter]);

  const filteredItems = filter
    ? items.filter((item) => {
        if (item.type === 'request') {
          return item.request.name.toLowerCase().includes(filter);
        }
        return true;
      })
    : items;

  if (filter && filteredItems.length === 0) return null;

  return (
    <div>
      <button
        type="button"
        className="flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
        onClick={() => setExpanded((prev) => !prev)}
        aria-expanded={expanded}
        aria-label={`${expanded ? 'Collapse' : 'Expand'} folder ${name}`}
      >
        {expanded ? (
          <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )}
        {expanded ? (
          <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )}
        <span className="truncate font-medium text-foreground">{name}</span>
      </button>
      {expanded && (
        <div className="pl-3">
          {filteredItems.map((item, idx) => {
            if (item.type === 'folder') {
              const folderPath = basePath ? `${basePath}/${item.folder.name}` : item.folder.name;
              return (
                <FolderNode
                  key={`folder-${folderPath}`}
                  name={item.folder.name}
                  items={item.folder.items}
                  collectionName={collectionName}
                  basePath={folderPath}
                  depth={depth + 1}
                  filter={filter}
                />
              );
            }
            const requestPath = basePath
              ? `${basePath}/${item.request.name}`
              : item.request.name;
            return (
              <RequestNode
                key={`request-${requestPath}-${idx}`}
                name={item.request.name}
                method={item.request.method}
                collectionName={collectionName}
                path={requestPath}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

// Renders a top-level collection as an expandable tree node.
function CollectionNode({
  summary,
  filter,
}: {
  summary: CollectionSummary;
  filter: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const [collection, setCollection] = useState<Collection | null>(null);

  // Fetch full collection data when expanded.
  useEffect(() => {
    if (expanded && !collection) {
      getCollection(summary.name)
        .then(setCollection)
        .catch((err) => console.error('[CollectionsSidebar] fetch error', err));
    }
  }, [expanded, collection, summary.name]);

  // Auto-expand when a filter is active.
  useEffect(() => {
    if (filter) setExpanded(true);
  }, [filter]);

  return (
    <div>
      <button
        type="button"
        className="flex items-center gap-1.5 w-full px-2 py-1.5 text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
        onClick={() => setExpanded((prev) => !prev)}
        aria-expanded={expanded}
        aria-label={`${expanded ? 'Collapse' : 'Expand'} collection ${summary.name}`}
      >
        {expanded ? (
          <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )}
        {expanded ? (
          <FolderOpen className="h-4 w-4 shrink-0 text-primary" />
        ) : (
          <Folder className="h-4 w-4 shrink-0 text-primary" />
        )}
        <span className="truncate font-medium text-foreground">{summary.name}</span>
        <span className="ml-auto text-[10px] text-muted-foreground">{summary.requestCount}</span>
      </button>
      {expanded && collection && (
        <div className="pl-2">
          {collection.root.items.map((item, idx) => {
            if (item.type === 'folder') {
              return (
                <FolderNode
                  key={`folder-${item.folder.name}`}
                  name={item.folder.name}
                  items={item.folder.items}
                  collectionName={summary.name}
                  basePath={item.folder.name}
                  depth={1}
                  filter={filter}
                />
              );
            }
            return (
              <RequestNode
                key={`request-${item.request.name}-${idx}`}
                name={item.request.name}
                method={item.request.method}
                collectionName={summary.name}
                path={item.request.name}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

// Sidebar panel with Collections tree and History tabs.
export function CollectionsSidebar() {
  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const filter = searchQuery.toLowerCase().trim();

  const fetchCollections = useCallback(async () => {
    try {
      const results = await listCollections();
      setSummaries(results);
    } catch (err) {
      console.error('[CollectionsSidebar] list error', err);
    }
  }, []);

  // Load collections on mount and subscribe to changes.
  useEffect(() => {
    void fetchCollections();
    let unlisten: (() => void) | undefined;
    onCollectionChanged(() => void fetchCollections()).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [fetchCollections]);

  return (
    <div className="h-full flex flex-col bg-card/50 backdrop-blur-sm border-r border-border/50">
      <Tabs defaultValue="collections" className="flex-1 flex flex-col">
        <TabsList className="mx-2 mt-2 mb-1 h-8 bg-muted/60">
          <TabsTrigger value="collections" className="text-xs px-3 py-1">
            Collections
          </TabsTrigger>
          <TabsTrigger value="history" className="text-xs px-3 py-1">
            History
          </TabsTrigger>
        </TabsList>

        <TabsContent value="collections" className="flex-1 flex flex-col mt-0 overflow-hidden">
          {/* Search and new collection. */}
          <div className="px-2 pb-2 space-y-1.5">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="h-7 pl-7 text-xs"
                placeholder="Search requests..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                aria-label="Search collections"
              />
            </div>
            <Button
              variant="ghost"
              size="sm"
              className="w-full justify-start h-7 text-xs text-muted-foreground hover:text-foreground"
            >
              <Plus className="h-3.5 w-3.5 mr-1.5" />
              New Collection
            </Button>
          </div>

          {/* Collection tree. */}
          <ScrollArea className="flex-1">
            <div className="px-1 pb-2">
              {summaries.length === 0 ? (
                <p className="px-3 py-4 text-center text-xs text-muted-foreground">
                  No collections yet.
                </p>
              ) : (
                summaries.map((s) => (
                  <CollectionNode key={s.name} summary={s} filter={filter} />
                ))
              )}
            </div>
          </ScrollArea>
        </TabsContent>

        <TabsContent value="history" className="flex-1 mt-0 overflow-hidden">
          <HistoryPanel />
        </TabsContent>
      </Tabs>
    </div>
  );
}
