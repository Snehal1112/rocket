import { useState } from 'react';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Folder, Search } from 'lucide-react';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import { mapApiRequestToState } from '@/lib/pane-utils';
import type { CollectionItem } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';

interface RequestListProps {
  items: CollectionItem[];
  collectionName: string;
}

// Maps each HTTP method to its badge color classes.
const METHOD_COLORS: Record<string, string> = {
  GET:     'text-emerald-500 border-emerald-500/30 bg-emerald-500/10',
  POST:    'text-amber-500   border-amber-500/30   bg-amber-500/10',
  PUT:     'text-blue-500    border-blue-500/30    bg-blue-500/10',
  PATCH:   'text-violet-500  border-violet-500/30  bg-violet-500/10',
  DELETE:  'text-red-500     border-red-500/30     bg-red-500/10',
  OPTIONS: 'text-cyan-500    border-cyan-500/30    bg-cyan-500/10',
  HEAD:    'text-pink-500    border-pink-500/30    bg-pink-500/10',
};

type RequestItem = Extract<CollectionItem, { type: 'request' }>;

interface Group {
  folderName: string | null;
  requests: RequestItem[];
}

// Collects requests into groups: root-level first, then one group per top-level folder.
function groupItems(items: CollectionItem[]): Group[] {
  const root: RequestItem[] = [];
  const folderGroups: Group[] = [];

  for (const item of items) {
    if (item.type === 'request') {
      root.push(item);
    } else {
      const requests = flattenRequests(item.items);
      folderGroups.push({ folderName: item.name, requests });
    }
  }

  const groups: Group[] = [];
  if (root.length > 0) groups.push({ folderName: null, requests: root });
  groups.push(...folderGroups);
  return groups;
}

// Recursively flattens all requests out of nested folder items.
function flattenRequests(items: CollectionItem[]): RequestItem[] {
  const results: RequestItem[] = [];
  for (const item of items) {
    if (item.type === 'request') {
      results.push(item);
    } else {
      results.push(...flattenRequests(item.items));
    }
  }
  return results;
}

// Checks whether a request matches the filter string by name or URL.
function matchesFilter(item: RequestItem, filter: string): boolean {
  const q = filter.toLowerCase();
  return item.name.toLowerCase().includes(q) || item.url.toLowerCase().includes(q);
}

export function RequestList({ items, collectionName }: RequestListProps) {
  const [filter, setFilter] = useState('');

  function openRequest(item: RequestItem, path: string) {
    const tab: RequestTab = {
      id: item.uid,
      title: item.name,
      tabType: 'request',
      request: mapApiRequestToState(item, true),
      response: null,
      isDirty: false,
      source: { collection: collectionName, path: item.fileName ?? path },
    };
    usePaneStore.getState().openTab(tab);
  }

  const groups = groupItems(items);

  // Filter groups — drop requests that don't match, drop empty groups.
  const filteredGroups = filter
    ? groups
        .map((g) => ({ ...g, requests: g.requests.filter((r) => matchesFilter(r, filter)) }))
        .filter((g) => g.requests.length > 0)
    : groups;

  const totalCount = groups.reduce((sum, g) => sum + g.requests.length, 0);

  return (
    <div className="flex flex-col gap-3">
      {/* Filter input. */}
      <div className="relative">
        <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
        <Input
          placeholder="Filter by name or URL…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="pl-8 h-8 text-sm"
        />
      </div>

      {/* Request count summary. */}
      <p className="text-xs text-muted-foreground">
        {totalCount} {totalCount === 1 ? 'request' : 'requests'}
      </p>

      {filteredGroups.length === 0 ? (
        <p className="text-xs text-muted-foreground py-4 text-center">No requests match your filter.</p>
      ) : (
        <div className="flex flex-col gap-4">
          {filteredGroups.map((group, gi) => (
            <div key={gi}>
              {/* Folder header — only shown for folder groups. */}
              {group.folderName !== null && (
                <div className="flex items-center gap-1.5 mb-1.5 px-1">
                  <Folder className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="text-xs font-medium text-muted-foreground">{group.folderName}</span>
                </div>
              )}

              {/* Request rows. */}
              <div className="rounded-md border overflow-hidden">
                {group.requests.map((req, ri) => {
                  const methodClass = METHOD_COLORS[req.method] ?? 'text-foreground border-border bg-muted';
                  return (
                    <button
                      key={req.uid}
                      type="button"
                      onClick={() => openRequest(req, req.name)}
                      className={cn(
                        'w-full flex items-center gap-3 px-3 py-2 text-left hover:bg-accent transition-colors',
                        ri > 0 && 'border-t border-border',
                      )}
                    >
                      <Badge
                        variant="outline"
                        className={cn('text-[10px] font-semibold w-14 justify-center shrink-0', methodClass)}
                      >
                        {req.method}
                      </Badge>
                      <span className="text-sm font-medium truncate min-w-0 flex-shrink-0 max-w-[30%]">
                        {req.name}
                      </span>
                      <span className="text-xs text-muted-foreground truncate min-w-0 flex-1">
                        {req.url}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
