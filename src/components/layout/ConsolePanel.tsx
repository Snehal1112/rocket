import { useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
import { statusTextColor } from '@/lib/colors';
import { useConsoleStore, type ConsoleEntry } from '@/stores/console-store';

interface ConsolePanelProps {
  isOpen: boolean;
  height: number;
  onHeightChange: (height: number) => void;
}

const MIN_HEIGHT = 120;
const MAX_HEIGHT = 600;

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
}

function EntryDetail({ entry }: { entry: ConsoleEntry }) {
  const sections = [
    { label: 'Request Headers', content: entry.requestHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)' },
    { label: 'Request Body', content: entry.requestBody || '(empty)' },
    { label: 'Response Headers', content: entry.responseHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)' },
    { label: 'Response Body', content: entry.responseBody || '(empty)' },
  ];

  return (
    <div className="grid grid-cols-2 gap-2 px-4 py-2 bg-muted/30 border-t text-xs">
      {sections.map((s) => (
        <div key={s.label}>
          <div className="font-medium text-muted-foreground mb-1">{s.label}</div>
          <pre className="font-mono whitespace-pre-wrap break-all bg-background/60 rounded p-1.5 max-h-32 overflow-auto text-2xs">
            {s.content}
          </pre>
        </div>
      ))}
    </div>
  );
}

export function ConsolePanel({ isOpen, height, onHeightChange }: ConsolePanelProps) {
  const entries = useConsoleStore((s) => s.entries);
  const clearEntries = useConsoleStore((s) => s.clearEntries);
  const [search, setSearch] = useState('');
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const dragRef = useRef<{ y: number; h: number } | null>(null);

  if (!isOpen) return null;

  const filtered = search
    ? entries.filter((e) => e.url.toLowerCase().includes(search.toLowerCase()))
    : entries;

  const handleDragDown = (e: React.PointerEvent) => {
    e.preventDefault();
    dragRef.current = { y: e.clientY, h: height };
    const onMove = (ev: PointerEvent) => {
      if (!dragRef.current) return;
      const delta = dragRef.current.y - ev.clientY;
      onHeightChange(Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, dragRef.current.h + delta)));
    };
    const onUp = () => {
      dragRef.current = null;
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  };

  return (
    <div
      className="shrink-0 border-t border-border/70 bg-card/85 backdrop-blur-sm flex flex-col"
      style={{ height }}
    >
      {/* Drag handle. */}
      <div
        className="h-1 cursor-row-resize bg-border/40 hover:bg-primary/40 transition-colors shrink-0"
        onPointerDown={handleDragDown}
      />

      {/* Toolbar. */}
      <div className="flex items-center gap-2 px-2 py-1 border-b border-border/70 shrink-0">
        <span className="text-sm font-medium">Console</span>
        {entries.length > 0 && (
          <span className="text-2xs px-1.5 py-0.5 rounded-full bg-muted text-muted-foreground">
            {entries.length}
          </span>
        )}
        <div className="flex-1" />
        <Input
          placeholder="Filter by URL"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="h-6 text-sm w-48"
        />
        <Button
          variant="ghost"
          size="sm"
          className="h-6 px-2 text-sm"
          onClick={clearEntries}
          aria-label="Clear console"
        >
          <Trash2 className="h-3.5 w-3.5 mr-1 text-muted-foreground" />
          Clear
        </Button>
      </div>

      {/* Entry list. */}
      <div className="flex-1 overflow-y-auto font-mono text-2xs">
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
            No requests sent yet
          </div>
        ) : (
          filtered.map((entry) => (
            <div key={entry.id}>
              <div
                className="flex items-center gap-1.5 px-2 py-1 hover:bg-accent/40 cursor-pointer border-b border-border/30"
                onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
              >
                {expandedId === entry.id
                  ? <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
                  : <ChevronRight className="h-3 w-3 shrink-0 text-muted-foreground" />
                }
                <span className="text-muted-foreground w-16 shrink-0">{formatTime(entry.timestamp)}</span>
                <span className="font-semibold w-12 shrink-0">{entry.method}</span>
                <span className="flex-1 truncate text-foreground/80">{entry.url}</span>
                <span className={cn('w-10 text-right shrink-0 font-semibold', statusTextColor(entry.status))}>
                  {entry.status || 'ERR'}
                </span>
                <span className="text-muted-foreground w-16 text-right shrink-0">
                  {entry.durationMs}ms
                </span>
              </div>
              {expandedId === entry.id && <EntryDetail entry={entry} />}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
