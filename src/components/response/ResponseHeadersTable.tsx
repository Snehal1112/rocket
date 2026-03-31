import { useState, useCallback } from 'react';
import { Copy, Check, Search } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';

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
        // Clipboard API not available -- silently ignore.
      }
    },
    [],
  );

  return (
    <div className="flex h-full flex-col gap-2">
      {/* Search input using shadcn Input. */}
      <div className="relative">
        <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          placeholder="Filter headers..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="h-7 pl-7 text-sm"
          aria-label="Filter headers"
        />
      </div>

      {/* Headers table using shadcn Table. */}
      {filtered.length === 0 ? (
        <p className="py-4 text-center text-xs text-muted-foreground">
          No headers{query ? ' match your filter' : ''}.
        </p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="h-7 text-xs font-semibold">Name</TableHead>
              <TableHead className="h-7 text-xs font-semibold">Value</TableHead>
              <TableHead className="h-7 w-8" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {filtered.map((h, i) => (
              <TableRow key={`${h.key}-${i}`}>
                <TableCell className="px-2 py-1.5 text-xs font-semibold text-foreground">
                  {h.key}
                </TableCell>
                <TableCell className="break-all px-2 py-1.5 font-mono text-xs text-muted-foreground">
                  {h.value}
                </TableCell>
                <TableCell className="px-1.5 py-1.5">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-5 w-5"
                    title="Copy value"
                    aria-label={`Copy value of ${h.key}`}
                    onClick={() => handleCopy(h.key, h.value)}
                  >
                    {copiedKey === h.key ? (
                      <Check className="h-3.5 w-3.5 text-emerald-500" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}
