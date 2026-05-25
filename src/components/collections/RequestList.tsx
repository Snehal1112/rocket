import { Folder, Search } from 'lucide-react';
import { useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { METHOD_BADGE_COLOR } from '@/lib/colors';
import { mapApiRequestToState } from '@/lib/pane-utils';
import type { CollectionItem } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

interface RequestListProps {
  items: CollectionItem[];
  collectionName: string;
}

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
    } else if (item.type === 'folder') {
      const requests = flattenRequests(item.items);
      folderGroups.push({ folderName: item.name, requests });
    }
    // summary items have no body — skip from this full-detail list view
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
    } else if (item.type === 'folder') {
      results.push(...flattenRequests(item.items));
    }
    // summary items have no body — omit from flat list
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
    <div className='flex flex-col gap-3'>
      {/* Filter input. */}
      <div className='relative'>
        <Search className='absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground' />
        <Input
          placeholder='Filter by name or URL…'
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className='pl-9 h-9 text-sm'
        />
      </div>

      {/* Request count summary. */}
      <p className='text-sm text-muted-foreground'>
        {totalCount} {totalCount === 1 ? 'request' : 'requests'}
      </p>

      {filteredGroups.length === 0 ? (
        <p className='text-xs text-muted-foreground py-4 text-center'>
          No requests match your filter.
        </p>
      ) : (
        <div className='flex flex-col gap-4'>
          {filteredGroups.map((group) => (
            <div key={group.folderName ?? 'root'}>
              {/* Folder header — only shown for folder groups. */}
              {group.folderName !== null && (
                <div className='flex items-center gap-1.5 mb-2 px-1'>
                  <Folder className='h-4 w-4 text-muted-foreground' />
                  <span className='text-sm font-medium text-muted-foreground'>
                    {group.folderName}
                  </span>
                </div>
              )}

              {/* Request rows. */}
              <div className='rounded-md border overflow-hidden'>
                {group.requests.map((req, ri) => {
                  const methodClass =
                    METHOD_BADGE_COLOR[req.method] ?? 'text-foreground border-border bg-muted';
                  return (
                    <button
                      key={req.uid}
                      type='button'
                      onClick={() => openRequest(req, req.name)}
                      className={cn(
                        'w-full flex items-center gap-3 px-3.5 py-2.5 text-left hover:bg-accent transition-colors',
                        ri > 0 && 'border-t border-border',
                      )}
                    >
                      <Badge
                        variant='outline'
                        className={cn(
                          'text-2xs font-semibold w-14 justify-center shrink-0',
                          methodClass,
                        )}
                      >
                        {req.method}
                      </Badge>
                      <span className='text-sm font-medium truncate min-w-0 flex-shrink-0 max-w-[30%]'>
                        {req.name}
                      </span>
                      <span className='text-sm text-muted-foreground truncate min-w-0 flex-1'>
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
