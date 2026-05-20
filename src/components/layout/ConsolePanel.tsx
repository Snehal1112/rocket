import { CheckCircle2, ChevronDown, ChevronRight, Trash2, XCircle } from 'lucide-react';
import { useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { statusTextColor } from '@/lib/colors';
import { cn } from '@/lib/utils';
import {
  type ConsoleEntry,
  type HttpConsoleEntry,
  type ScriptLogEntry,
  type TestResultEntry,
  useConsoleStore,
} from '@/stores/console-store';

interface ConsolePanelProps {
  isOpen: boolean;
  height: number;
  onHeightChange: (height: number) => void;
}

const MIN_HEIGHT = 120;
const MAX_HEIGHT = 600;

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

function HttpEntryDetail({ entry }: { entry: HttpConsoleEntry }) {
  const sections = [
    {
      label: 'Request Headers',
      content: entry.requestHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)',
    },
    { label: 'Request Body', content: entry.requestBody || '(empty)' },
    {
      label: 'Response Headers',
      content: entry.responseHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)',
    },
    { label: 'Response Body', content: entry.responseBody || '(empty)' },
  ];

  return (
    <div className='grid grid-cols-2 gap-2 px-4 py-2 bg-muted/30 border-t text-xs'>
      {sections.map((s) => (
        <div key={s.label}>
          <div className='font-medium text-muted-foreground mb-1'>{s.label}</div>
          <pre className='font-mono whitespace-pre-wrap break-all bg-background/60 rounded p-1.5 max-h-32 overflow-auto text-2xs'>
            {s.content}
          </pre>
        </div>
      ))}
    </div>
  );
}

const scriptLevelColor: Record<ScriptLogEntry['level'], string> = {
  log: 'text-foreground/80',
  warn: 'text-yellow-500',
  error: 'text-red-500',
};

const scriptLevelLabel: Record<ScriptLogEntry['level'], string> = {
  log: 'log',
  warn: 'warn',
  error: 'err',
};

function ScriptLogRow({ entry }: { entry: ScriptLogEntry }) {
  return (
    <div className='flex items-start gap-1.5 px-2 py-1 border-b border-border/30'>
      {/* Spacer matching the chevron width used in HTTP rows. */}
      <span className='w-3.5 shrink-0' />
      <span className='text-muted-foreground w-16 shrink-0'>{formatTime(entry.timestamp)}</span>
      <span
        className={cn(
          'font-semibold w-12 shrink-0 uppercase text-2xs',
          scriptLevelColor[entry.level],
        )}
      >
        {scriptLevelLabel[entry.level]}
      </span>
      <span className='text-muted-foreground shrink-0 truncate max-w-[8rem]'>
        {entry.requestName}
      </span>
      <span className={cn('flex-1 break-all', scriptLevelColor[entry.level])}>{entry.message}</span>
    </div>
  );
}

function TestResultRow({ entry }: { entry: TestResultEntry }) {
  return (
    <div className='flex items-start gap-1.5 px-2 py-1 border-b border-border/30'>
      <span className='w-3.5 shrink-0' />
      <span className='text-muted-foreground w-16 shrink-0'>{formatTime(entry.timestamp)}</span>
      {entry.status === 'passed' ? (
        <CheckCircle2 className='h-3.5 w-3.5 text-green-500 mt-0.5 shrink-0' />
      ) : (
        <XCircle className='h-3.5 w-3.5 text-red-500 mt-0.5 shrink-0' />
      )}
      <span className='text-muted-foreground shrink-0 truncate max-w-[8rem]'>
        {entry.requestName}
      </span>
      <span
        className={cn(
          'flex-1 break-all',
          entry.status === 'passed' ? 'text-green-500' : 'text-red-400',
        )}
      >
        {entry.name}
        {entry.error && <span className='block text-red-400 font-mono mt-0.5'>{entry.error}</span>}
      </span>
    </div>
  );
}

function matchesSearch(entry: ConsoleEntry, term: string): boolean {
  const lower = term.toLowerCase();
  if (entry.kind === 'http') return entry.url.toLowerCase().includes(lower);
  if (entry.kind === 'test')
    return (
      entry.name.toLowerCase().includes(lower) || entry.requestName.toLowerCase().includes(lower)
    );
  return (
    entry.message.toLowerCase().includes(lower) || entry.requestName.toLowerCase().includes(lower)
  );
}

export function ConsolePanel({ isOpen, height, onHeightChange }: ConsolePanelProps) {
  const entries = useConsoleStore((s) => s.entries);
  const clearEntries = useConsoleStore((s) => s.clearEntries);
  const [search, setSearch] = useState('');
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const dragRef = useRef<{ y: number; h: number } | null>(null);

  if (!isOpen) return null;

  const filtered = search ? entries.filter((e) => matchesSearch(e, search)) : entries;

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
      className='shrink-0 border-t border-panel-border bg-panel-bg flex flex-col'
      style={{ height }}
    >
      {/* Drag handle. */}
      <div
        className='h-1 cursor-row-resize bg-border/40 hover:bg-primary/40 transition-colors shrink-0'
        onPointerDown={handleDragDown}
      />

      {/* Toolbar. */}
      <div className='flex items-center gap-2 px-2 py-1 border-b border-border/70 shrink-0'>
        <span className='text-sm font-medium'>Console</span>
        {entries.length > 0 && (
          <span className='text-2xs px-1.5 py-0.5 rounded-full bg-muted text-muted-foreground'>
            {entries.length}
          </span>
        )}
        <div className='flex-1' />
        <Input
          placeholder='Filter by URL or message'
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className='h-6 text-sm w-48'
        />
        <Button
          variant='ghost'
          size='sm'
          className='h-6 px-2 text-sm'
          onClick={clearEntries}
          aria-label='Clear console'
        >
          <Trash2 className='h-3.5 w-3.5 mr-1 text-muted-foreground' />
          Clear
        </Button>
      </div>

      {/* Entry list. */}
      <div className='flex-1 overflow-y-auto font-mono text-2xs'>
        {filtered.length === 0 ? (
          <div className='flex items-center justify-center h-full text-muted-foreground text-sm'>
            No requests sent yet
          </div>
        ) : (
          filtered.map((entry) => {
            if (entry.kind === 'script') {
              return <ScriptLogRow key={entry.id} entry={entry} />;
            }
            if (entry.kind === 'test') {
              return <TestResultRow key={entry.id} entry={entry} />;
            }

            return (
              <div key={entry.id}>
                <button
                  type='button'
                  className='flex items-center gap-1.5 px-2 py-1 hover:bg-accent/40 cursor-pointer border-b border-border/30 w-full text-left'
                  onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      setExpandedId(expandedId === entry.id ? null : entry.id);
                    }
                  }}
                >
                  {expandedId === entry.id ? (
                    <ChevronDown className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
                  ) : (
                    <ChevronRight className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
                  )}
                  <span className='text-muted-foreground w-16 shrink-0'>
                    {formatTime(entry.timestamp)}
                  </span>
                  <span className='font-semibold w-12 shrink-0'>{entry.method}</span>
                  <span className='flex-1 truncate text-foreground/80'>{entry.url}</span>
                  <span
                    className={cn(
                      'w-10 text-right shrink-0 font-semibold',
                      statusTextColor(entry.status),
                    )}
                  >
                    {entry.status || 'ERR'}
                  </span>
                  <span className='text-muted-foreground w-16 text-right shrink-0'>
                    {entry.durationMs}ms
                  </span>
                </button>
                {expandedId === entry.id && <HttpEntryDetail entry={entry} />}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
