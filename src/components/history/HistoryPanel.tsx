import { useEffect, useRef, useState, useCallback } from 'react';
import { Search } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
import { listHistory, searchHistory } from '@/lib/tauri-api';
import type { HistoryEntry } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { createDefaultRequest } from '@/lib/pane-utils';
import type { Tab } from '@/types/pane-types';

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

// Returns Tailwind text color for an HTTP status code.
function statusColor(status: number): string {
  if (status >= 200 && status < 300) return 'text-emerald-500';
  if (status >= 300 && status < 400) return 'text-blue-500';
  if (status >= 400 && status < 500) return 'text-amber-500';
  if (status >= 500) return 'text-red-500';
  return 'text-muted-foreground';
}

// Returns Tailwind text color for an HTTP method badge.
function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case 'GET':    return 'text-emerald-500';
    case 'POST':   return 'text-amber-500';
    case 'PUT':    return 'text-blue-500';
    case 'PATCH':  return 'text-violet-500';
    case 'DELETE': return 'text-red-500';
    default:       return 'text-muted-foreground';
  }
}

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

  // Debounce timer ref so we can clear it on each keystroke.
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Fetches history with current filters applied.
  const fetchEntries = useCallback(
    async (url: string, m: MethodOption, sl: StatusLabel) => {
      const statusRange = STATUS_OPTIONS.find((s) => s.label === sl);
      try {
        if (!url && m === 'All' && sl === 'All') {
          const results = await listHistory(200);
          setEntries(results);
        } else {
          const results = await searchHistory({
            urlContains: url || undefined,
            method: m !== 'All' ? m : undefined,
            statusMin: statusRange?.min,
            statusMax: statusRange?.max,
          });
          setEntries(results);
        }
      } catch (err) {
        console.error('[HistoryPanel] fetch error', err);
      }
    },
    [],
  );

  // Load recent history on mount.
  useEffect(() => {
    void fetchEntries('', 'All', 'All');
  }, [fetchEntries]);

  // Re-fetch when method or status filter changes immediately.
  useEffect(() => {
    void fetchEntries(urlQuery, method, statusLabel);
    // urlQuery intentionally excluded — debounced separately below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [method, statusLabel]);

  // Debounced URL search with 300 ms delay.
  function handleUrlChange(value: string) {
    setUrlQuery(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      void fetchEntries(value, method, statusLabel);
    }, 300);
  }

  // Opens a history entry as a read-only history tab in the active pane.
  function openEntry(entry: HistoryEntry) {
    const tab: Tab = {
      id: `history-${entry.id}`,
      title: `${entry.method} ${displayUrl(entry.url)}`,
      tabType: 'history',
      request: {
        ...createDefaultRequest(),
        method: entry.method as Tab['request']['method'],
        url: entry.url,
      },
      response: null,
      isDirty: false,
    };
    usePaneStore.getState().openTab(tab);
  }

  return (
    <div className="flex h-full flex-col">
      {/* Search and filter bar. */}
      <div className="space-y-2 border-b border-border p-2">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="h-7 pl-7 text-xs"
            placeholder="Search URL..."
            value={urlQuery}
            onChange={(e) => handleUrlChange(e.target.value)}
            aria-label="Search history by URL"
          />
        </div>

        <div className="flex gap-2">
          {/* Method filter. */}
          <select
            className="h-7 flex-1 rounded-md border border-input bg-transparent px-2 text-xs shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
            value={method}
            onChange={(e) => setMethod(e.target.value as MethodOption)}
            aria-label="Filter by HTTP method"
          >
            {METHOD_OPTIONS.map((m) => (
              <option key={m} value={m}>
                {m === 'All' ? 'All methods' : m}
              </option>
            ))}
          </select>

          {/* Status filter. */}
          <select
            className="h-7 flex-1 rounded-md border border-input bg-transparent px-2 text-xs shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
            value={statusLabel}
            onChange={(e) => setStatusLabel(e.target.value as StatusLabel)}
            aria-label="Filter by status code"
          >
            {STATUS_OPTIONS.map((s) => (
              <option key={s.label} value={s.label}>
                {s.label === 'All' ? 'All statuses' : s.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Entry list. */}
      <div className="flex-1 overflow-y-auto">
        {entries.length === 0 ? (
          <p className="p-4 text-center text-xs text-muted-foreground">
            No history yet. Execute a request to see it here.
          </p>
        ) : (
          <ul>
            {entries.map((entry) => (
              <li key={entry.id}>
                <button
                  type="button"
                  className="w-full cursor-pointer px-3 py-2 text-left hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  onClick={() => openEntry(entry)}
                  aria-label={`Open ${entry.method} ${entry.url}`}
                >
                  {/* Top row: method badge + truncated URL. */}
                  <div className="flex items-center gap-2">
                    <span
                      className={cn(
                        'w-12 shrink-0 text-xs font-semibold',
                        methodColor(entry.method),
                      )}
                    >
                      {entry.method}
                    </span>
                    <span className="truncate text-xs text-foreground">
                      {displayUrl(entry.url)}
                    </span>
                  </div>

                  {/* Bottom row: status, duration, timestamp. */}
                  <div className="mt-0.5 flex items-center gap-2 pl-14">
                    <span
                      className={cn('text-xs font-medium', statusColor(entry.status))}
                    >
                      {entry.status}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {entry.durationMs}ms
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {formatTime(entry.timestamp)}
                    </span>
                  </div>
                </button>
                <div className="mx-3 h-px bg-border" />
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
