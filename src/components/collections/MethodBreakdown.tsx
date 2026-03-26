import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { cn } from '@/lib/utils';
import type { CollectionItem } from '@/lib/tauri-api';

interface MethodBreakdownProps {
  items: CollectionItem[];
}

// Maps each HTTP method to its text and background color classes.
const METHOD_COLORS: Record<string, { text: string; bg: string }> = {
  GET:     { text: 'text-emerald-500', bg: 'bg-emerald-500' },
  POST:    { text: 'text-amber-500',   bg: 'bg-amber-500' },
  PUT:     { text: 'text-blue-500',    bg: 'bg-blue-500' },
  PATCH:   { text: 'text-violet-500',  bg: 'bg-violet-500' },
  DELETE:  { text: 'text-red-500',     bg: 'bg-red-500' },
  OPTIONS: { text: 'text-cyan-500',    bg: 'bg-cyan-500' },
  HEAD:    { text: 'text-pink-500',    bg: 'bg-pink-500' },
};

// Recursively counts HTTP methods across all requests, including nested folders.
function countMethods(items: CollectionItem[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const item of items) {
    if (item.type === 'request') {
      counts[item.method] = (counts[item.method] || 0) + 1;
    } else {
      const sub = countMethods(item.items);
      for (const [k, v] of Object.entries(sub)) counts[k] = (counts[k] || 0) + v;
    }
  }
  return counts;
}

export function MethodBreakdown({ items }: MethodBreakdownProps) {
  const counts = countMethods(items);
  const total = Object.values(counts).reduce((sum, n) => sum + n, 0);

  // Only show methods that are actually used.
  const rows = Object.entries(counts)
    .filter(([, count]) => count > 0)
    .sort(([, a], [, b]) => b - a);

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-medium">Method Breakdown</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {rows.length === 0 ? (
          <p className="text-xs text-muted-foreground">No requests in this collection.</p>
        ) : (
          rows.map(([method, count]) => {
            const color = METHOD_COLORS[method] ?? { text: 'text-foreground', bg: 'bg-foreground' };
            const pct = total > 0 ? Math.round((count / total) * 100) : 0;
            return (
              <div key={method} className="flex items-center gap-3">
                <span className={cn('w-16 text-xs font-semibold', color.text)}>{method}</span>
                <div className="flex-1 h-2 bg-muted rounded-full overflow-hidden">
                  <div
                    className={cn('h-full rounded-full w-[var(--bar-w)]', color.bg)}
                    style={{ '--bar-w': `${pct}%` } as React.CSSProperties}
                  />
                </div>
                <span className="text-xs text-muted-foreground w-16 text-right">
                  {count} ({pct}%)
                </span>
              </div>
            );
          })
        )}
      </CardContent>
    </Card>
  );
}
