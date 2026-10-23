import { Search } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { METHOD_TEXT_COLOR, statusTextColor } from '@/lib/colors';
import { createDefaultRequest } from '@/lib/pane-utils';
import type { HistoryEntry, RunSource } from '@/lib/tauri-api';
import { listHistory, searchHistory } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { RequestTab } from '@/types/pane-types';

// HTTP method filter options.
const METHOD_OPTIONS = ['All', 'GET', 'POST', 'PUT', 'PATCH', 'DELETE'] as const;
type MethodOption = (typeof METHOD_OPTIONS)[number];

// Status class filter options mapped to min/max HTTP status ranges.
const STATUS_OPTIONS = [
  { label: 'All', min: undefined, max: undefined },
  { label: '2xx', min: 200, max: 299 },
  { label: '3xx', min: 300, max: 399 },
  { label: '4xx', min: 400, max: 499 },
  { label: '5xx', min: 500, max: 599 },
] as const;
type StatusLabel = (typeof STATUS_OPTIONS)[number]['label'];

// Run source filter options. Manual entries get no badge in the list.
const SOURCE_OPTIONS: { value: 'all' | RunSource; label: string }[] = [
  { value: 'all', label: 'All sources' },
  { value: 'manual', label: 'Manual' },
  { value: 'runner', label: 'Runner' },
  { value: 'load_test', label: 'Load test' },
  { value: 'flow', label: 'Flow' },
  { value: 'agent', label: 'Agent' },
];
type SourceOption = (typeof SOURCE_OPTIONS)[number]['value'];

// Badge labels for the non-manual sources.
const SOURCE_BADGE: Partial<Record<RunSource, string>> = {
  runner: 'Runner',
  load_test: 'Load test',
  flow: 'Flow',
  agent: 'Agent',
};

// Formats an ISO timestamp to a short local time string (e.g. "12:34 PM").
function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString(undefined, {
      hour: 'numeric',
      minute: '2-digit',
    });
  } catch {
    return '';
  }
}

// Extracts the host + path portion of a URL for display.
function displayUrl(raw: string): string {
  try {
    const u = new URL(raw);
    return u.host + u.pathname;
  } catch {
    return raw;
  }
}

export function HistoryPanel() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [urlQuery, setUrlQuery] = useState('');
  const [method, setMethod] = useState<MethodOption>('All');
  const [statusLabel, setStatusLabel] = useState<StatusLabel>('All');
  const [source, setSource] = useState<SourceOption>('all');
  // True when the last load failed, so an empty list is not shown as "no history".
  const [loadFailed, setLoadFailed] = useState(false);

  // Debounce timer ref so we can clear it on each keystroke.
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Fetches history with current filters applied.
  const fetchEntries = useCallback(
    async (url: string, m: MethodOption, sl: StatusLabel, src: SourceOption) => {
      const statusRange = STATUS_OPTIONS.find((s) => s.label === sl);
      try {
        if (!url && m === 'All' && sl === 'All' && src === 'all') {
          const results = await listHistory(200);
          setEntries(results);
        } else {
          const results = await searchHistory({
            urlContains: url || undefined,
            method: m !== 'All' ? m : undefined,
            statusMin: statusRange?.min,
            statusMax: statusRange?.max,
            runSource: src !== 'all' ? src : undefined,
          });
          setEntries(results);
        }
        setLoadFailed(false);
      } catch (err) {
        console.error('[HistoryPanel] fetch error', err);
        setLoadFailed(true);
      }
    },
    [],
  );

  // The panel stays mounted, so it reloads when the workspace changes.
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);

  // Load on mount, and re-fetch immediately when method, status or workspace changes.
  // (urlQuery is excluded — debounced separately in handleUrlChange below.)
  // biome-ignore lint/correctness/useExhaustiveDependencies: urlQuery excluded intentionally, debounced in handleUrlChange; activeWorkspaceId only triggers a reload
  useEffect(() => {
    void fetchEntries(urlQuery, method, statusLabel, source);
  }, [fetchEntries, method, statusLabel, source, activeWorkspaceId]);

  // Debounced URL search with 300 ms delay.
  function handleUrlChange(value: string) {
    setUrlQuery(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      void fetchEntries(value, method, statusLabel, source);
    }, 300);
  }

  const hasFilters =
    urlQuery !== '' || method !== 'All' || statusLabel !== 'All' || source !== 'all';

  // Resets every filter and reloads the full list.
  function clearFilters() {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    setUrlQuery('');
    setMethod('All');
    setStatusLabel('All');
    setSource('all');
    void fetchEntries('', 'All', 'All', 'all');
  }

  // Opens a history entry as a read-only history tab in the active pane.
  function openEntry(entry: HistoryEntry) {
    const tab: RequestTab = {
      id: `history-${entry.id}`,
      title: `${entry.method} ${displayUrl(entry.url)}`,
      tabType: 'history',
      request: {
        ...createDefaultRequest(),
        method: entry.method as RequestTab['request']['method'],
        url: entry.url,
      },
      response: null,
      isDirty: false,
    };
    usePaneStore.getState().openTab(tab);
  }

  return (
    <div className='flex h-full flex-col'>
      {/* Search and filter bar. */}
      <div className='space-y-2 border-b border-border p-2'>
        <div className='relative'>
          <Search className='absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground' />
          <Input
            className='h-7 pl-7 text-sm'
            placeholder='Search URL...'
            value={urlQuery}
            onChange={(e) => handleUrlChange(e.target.value)}
            aria-label='Search history by URL'
          />
        </div>

        <div className='flex gap-2'>
          {/* Method filter. */}
          <select
            className='h-7 flex-1 rounded-md border border-input bg-transparent px-2 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'
            value={method}
            onChange={(e) => setMethod(e.target.value as MethodOption)}
            aria-label='Filter by HTTP method'
          >
            {METHOD_OPTIONS.map((m) => (
              <option key={m} value={m}>
                {m === 'All' ? 'All methods' : m}
              </option>
            ))}
          </select>

          {/* Status filter. */}
          <select
            className='h-7 flex-1 rounded-md border border-input bg-transparent px-2 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'
            value={statusLabel}
            onChange={(e) => setStatusLabel(e.target.value as StatusLabel)}
            aria-label='Filter by status code'
          >
            {STATUS_OPTIONS.map((s) => (
              <option key={s.label} value={s.label}>
                {s.label === 'All' ? 'All statuses' : s.label}
              </option>
            ))}
          </select>

          {/* Run source filter. */}
          <Select value={source} onValueChange={(v) => setSource(v as SourceOption)}>
            <SelectTrigger className='h-7 flex-1 px-2' aria-label='Filter by run source'>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SOURCE_OPTIONS.map((o) => (
                <SelectItem key={o.value} value={o.value}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Entry list. */}
      <div className='flex-1 overflow-y-auto'>
        {entries.length === 0 ? (
          loadFailed ? (
            <EmptyState
              size='compact'
              cause='unavailable'
              title="Couldn't load history"
              reason='Check the Console for details, then try again.'
              action={{
                label: 'Try again',
                onClick: () => void fetchEntries(urlQuery, method, statusLabel, source),
              }}
            />
          ) : hasFilters ? (
            <EmptyState
              size='compact'
              cause='filtered'
              title='No matching history'
              reason='No request in your history matches these filters.'
              action={{ label: 'Clear filters', onClick: clearFilters }}
            />
          ) : (
            <EmptyState
              size='compact'
              cause='none-yet'
              title='No history yet'
              reason='Requests you send appear here.'
            />
          )
        ) : (
          <ul>
            {entries.map((entry) => (
              <li key={entry.id}>
                <button
                  type='button'
                  className='w-full cursor-pointer px-2 py-1 text-left hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring'
                  onClick={() => openEntry(entry)}
                  aria-label={`Open ${entry.method} ${entry.url}`}
                >
                  {/* Top row: method badge + truncated URL. */}
                  <div className='flex items-center gap-1'>
                    <span
                      className={cn(
                        'w-12 shrink-0 text-xs font-semibold',
                        METHOD_TEXT_COLOR[entry.method.toUpperCase()] ?? 'text-muted-foreground',
                      )}
                    >
                      {entry.method}
                    </span>
                    <span className='truncate text-sm text-foreground'>
                      {displayUrl(entry.url)}
                    </span>
                  </div>

                  {/* Bottom row: status, duration, timestamp. */}
                  <div className='mt-0.5 flex items-center gap-1.5 pl-14'>
                    <span className={cn('text-xs font-medium', statusTextColor(entry.status))}>
                      {entry.status}
                    </span>
                    <span className='text-xs text-muted-foreground'>{entry.durationMs}ms</span>
                    <span className='text-xs text-muted-foreground'>
                      {formatTime(entry.timestamp)}
                    </span>
                    {entry.runSource && SOURCE_BADGE[entry.runSource] && (
                      <Badge variant='outline' className='px-1.5 py-0 text-[10px] font-medium'>
                        {SOURCE_BADGE[entry.runSource]}
                      </Badge>
                    )}
                  </div>
                </button>
                <div className='mx-2 h-px bg-border' />
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
