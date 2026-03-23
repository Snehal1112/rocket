import { useState, useCallback } from 'react';
import { Copy, Check, Search } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface ResponseHeadersTableProps {
  headers: { key: string; value: string; enabled: boolean }[];
}

// Displays response headers with search filtering and clipboard copy.
export function ResponseHeadersTable({ headers }: ResponseHeadersTableProps) {
  const [query, setQuery] = useState('');
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const filtered = headers.filter((h) => {
    if (!query) return true;
    const q = query.toLowerCase();
    return h.key.toLowerCase().includes(q) || h.value.toLowerCase().includes(q);
  });

  const handleCopy = useCallback(
    async (key: string, value: string) => {
      try {
        await navigator.clipboard.writeText(value);
        setCopiedKey(key);
        setTimeout(() => setCopiedKey((prev) => (prev === key ? null : prev)), 1000);
      } catch {
        // Clipboard API not available — silently ignore.
      }
    },
    [],
  );

  return (
    <div className="flex h-full flex-col gap-2">
      {/* Search input. */}
      <div className="relative">
        <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
        <input
          type="text"
          placeholder="Filter headers…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className={cn(
            'h-7 w-full rounded-md border border-input bg-background pl-7 pr-3 text-xs',
            'placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring',
          )}
          aria-label="Filter headers"
        />
      </div>

      {/* Headers table. */}
      {filtered.length === 0 ? (
        <p className="py-4 text-center text-xs text-muted-foreground">
          No headers{query ? ' match your filter' : ''}.
        </p>
      ) : (
        <div className="overflow-auto rounded-md border border-border">
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-border bg-muted/50">
                <th className="px-3 py-1.5 text-left font-semibold text-muted-foreground">
                  Name
                </th>
                <th className="px-3 py-1.5 text-left font-semibold text-muted-foreground">
                  Value
                </th>
                <th className="w-8" />
              </tr>
            </thead>
            <tbody>
              {filtered.map((h, i) => (
                <tr
                  key={`${h.key}-${i}`}
                  className="border-b border-border last:border-0 hover:bg-muted/30"
                >
                  <td className="px-3 py-1.5 font-semibold text-foreground">
                    {h.key}
                  </td>
                  <td className="break-all px-3 py-1.5 font-mono text-muted-foreground">
                    {h.value}
                  </td>
                  <td className="px-1.5 py-1.5">
                    <button
                      type="button"
                      title="Copy value"
                      aria-label={`Copy value of ${h.key}`}
                      onClick={() => handleCopy(h.key, h.value)}
                      className={cn(
                        'flex h-5 w-5 items-center justify-center rounded transition-colors',
                        copiedKey === h.key
                          ? 'text-emerald-500'
                          : 'text-muted-foreground hover:text-foreground',
                      )}
                    >
                      {copiedKey === h.key ? (
                        <Check className="h-3 w-3" />
                      ) : (
                        <Copy className="h-3 w-3" />
                      )}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
