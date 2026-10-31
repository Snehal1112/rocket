import {
  Braces,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Eye,
  EyeOff,
  FlaskConical,
  Globe,
  Trash2,
  TriangleAlert,
  XCircle,
} from 'lucide-react';
import { useMemo, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { statusTextColor } from '@/lib/colors';
import { hasHiddenValues, selectHttpView } from '@/lib/console-redaction';
import { cn } from '@/lib/utils';
import {
  type BackendLogLevel,
  type ConsoleEntry,
  type HttpConsoleEntry,
  type LogConsoleEntry,
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

// What the Console collects, so an empty panel says what will show up.
const CONSOLE_SOURCES = [
  { label: 'Requests', Icon: Globe },
  { label: 'Script output', Icon: Braces },
  { label: 'Test results', Icon: FlaskConical },
  { label: 'Backend warnings', Icon: TriangleAlert },
] as const;

function ConsoleEmptyState() {
  return (
    <div
      role='status'
      data-slot='empty-state'
      data-cause='none-yet'
      className='flex h-full flex-col items-center justify-center gap-3 px-4 py-3 text-center font-sans'
    >
      <div className='space-y-0.5'>
        <p className='text-sm font-medium text-foreground'>No console activity yet</p>
        <p className='text-xs text-muted-foreground'>
          Send a request or run a script to see it here.
        </p>
      </div>
      <ul aria-label='What the Console shows' className='flex flex-wrap justify-center gap-1.5'>
        {CONSOLE_SOURCES.map(({ label, Icon }) => (
          <li
            key={label}
            className='inline-flex items-center gap-1 rounded-full border border-border/70 px-2 py-0.5 text-xs text-muted-foreground'
          >
            <Icon aria-hidden='true' className='h-3 w-3' />
            {label}
          </li>
        ))}
      </ul>
    </div>
  );
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

interface HttpEntryDetailProps {
  entry: HttpConsoleEntry;
  revealed: boolean;
  onToggleReveal: () => void;
}

function HttpEntryDetail({ entry, revealed, onToggleReveal }: HttpEntryDetailProps) {
  const view = selectHttpView(entry, revealed);
  const canReveal = !entry.uncertain && hasHiddenValues(entry);
  const sections = [
    {
      label: 'Request Headers',
      content: view.requestHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)',
    },
    { label: 'Request Body', content: view.requestBody || '(empty)' },
    {
      label: 'Response Headers',
      content: view.responseHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)',
    },
    { label: 'Response Body', content: view.responseBody || '(empty)' },
  ];

  return (
    <div className='bg-muted/30 border-t text-xs'>
      {(canReveal || entry.uncertain) && (
        <div className='flex items-center justify-end gap-2 px-4 pt-2'>
          {entry.uncertain && <span className='text-muted-foreground'>May contain secrets</span>}
          {canReveal && (
            <Button
              type='button'
              variant='ghost'
              size='sm'
              className='h-6 gap-1 px-2 text-xs'
              aria-label={revealed ? 'Hide secrets' : 'Show secrets'}
              onClick={onToggleReveal}
            >
              {revealed ? <EyeOff className='h-3.5 w-3.5' /> : <Eye className='h-3.5 w-3.5' />}
              {revealed ? 'Hide secrets' : 'Show secrets'}
            </Button>
          )}
        </div>
      )}
      <div className='grid grid-cols-2 gap-2 px-4 py-2'>
        {sections.map((s) => (
          <div key={s.label}>
            <div className='font-medium text-muted-foreground mb-1'>{s.label}</div>
            <pre className='font-mono whitespace-pre-wrap break-all bg-elevation-sunken rounded p-1.5 max-h-32 overflow-auto text-2xs'>
              {s.content}
            </pre>
          </div>
        ))}
      </div>
    </div>
  );
}

const scriptLevelColor: Record<ScriptLogEntry['level'], string> = {
  log: 'text-foreground/80',
  warn: 'text-yellow-500',
  error: 'text-destructive',
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
        <XCircle className='h-3.5 w-3.5 text-destructive mt-0.5 shrink-0' />
      )}
      <span className='text-muted-foreground shrink-0 truncate max-w-[8rem]'>
        {entry.requestName}
      </span>
      <span
        className={cn(
          'flex-1 break-all',
          entry.status === 'passed' ? 'text-green-500' : 'text-destructive',
        )}
      >
        {entry.name}
        {entry.error && (
          <span className='block text-destructive font-mono mt-0.5'>{entry.error}</span>
        )}
      </span>
    </div>
  );
}

type PanelEntry = ConsoleEntry | LogConsoleEntry;

const logLevelColor: Record<BackendLogLevel, string> = {
  INFO: 'text-sky-500',
  WARN: 'text-yellow-500',
  ERROR: 'text-destructive',
};

function LogRow({ entry }: { entry: LogConsoleEntry }) {
  const fields = Object.entries({ ...entry.spanFields, ...entry.fields });
  return (
    <div data-testid='console-log-row' className='px-2 py-1 border-b border-border/30'>
      <div className='flex items-start gap-1.5'>
        <span className='w-3.5 shrink-0' />
        <span className='text-muted-foreground w-16 shrink-0'>{formatTime(entry.timestamp)}</span>
        <span
          data-testid='console-log-level'
          className={cn(
            'font-semibold w-12 shrink-0 text-center rounded border border-current text-2xs',
            logLevelColor[entry.level],
          )}
        >
          {entry.level}
        </span>
        <span className='text-muted-foreground shrink-0 truncate max-w-[12rem]'>
          {entry.target}
        </span>
        <span className='flex-1 break-all text-foreground/80'>{entry.message}</span>
      </div>
      {fields.length > 0 && (
        <div className='pl-[8.5rem] text-muted-foreground break-all'>
          {fields.map(([k, v]) => `${k}=${v}`).join(' ')}
        </div>
      )}
    </div>
  );
}

function matchesSearch(entry: PanelEntry, term: string, revealedId: string | null): boolean {
  const lower = term.toLowerCase();
  if (entry.kind === 'log')
    return (
      entry.message.toLowerCase().includes(lower) || entry.target.toLowerCase().includes(lower)
    );
  if (entry.kind === 'http')
    return selectHttpView(entry, entry.id === revealedId)
      .url.toLowerCase()
      .includes(lower);
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
  const logEntries = useConsoleStore((s) => s.logEntries);
  const clearEntries = useConsoleStore((s) => s.clearEntries);
  const [showInfoLogs, setShowInfoLogs] = useState(false);
  const [search, setSearch] = useState('');
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [revealedId, setRevealedId] = useState<string | null>(null);
  // Opening, closing or switching rows always masks again.
  const toggleExpanded = (id: string) => {
    setRevealedId(null);
    setExpandedId((current) => (current === id ? null : id));
  };
  const dragRef = useRef<{ y: number; h: number } | null>(null);

  // Backend logs show WARN and ERROR by default; INFO is opt-in.
  // Agent tool lines are INFO but always show, so agent actions stay visible.
  const visible = useMemo(() => {
    const logs = showInfoLogs
      ? logEntries
      : logEntries.filter((l) => l.level !== 'INFO' || l.target === 'agent');
    if (logs.length === 0) return entries as PanelEntry[];
    const all: PanelEntry[] = [...entries, ...logs];
    return all.sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp));
  }, [entries, logEntries, showInfoLogs]);

  if (!isOpen) return null;

  const filtered = search ? visible.filter((e) => matchesSearch(e, search, revealedId)) : visible;
  // INFO logs the switch hides, so an empty list can say they exist.
  const hiddenInfoCount = showInfoLogs
    ? 0
    : logEntries.filter((l) => l.level === 'INFO' && l.target !== 'agent').length;

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
        {visible.length > 0 && (
          <span className='text-2xs px-1.5 py-0.5 rounded-full bg-muted text-muted-foreground'>
            {visible.length}
          </span>
        )}
        <div className='flex-1' />
        <div className='flex items-center gap-1.5'>
          <Switch
            id='console-show-info-logs'
            checked={showInfoLogs}
            onCheckedChange={setShowInfoLogs}
            aria-label='Show INFO backend logs'
          />
          <span className='text-sm text-muted-foreground'>Show INFO logs</span>
        </div>
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
          disabled={entries.length === 0 && logEntries.length === 0}
          aria-label='Clear console'
        >
          <Trash2 className='h-3.5 w-3.5 mr-1 text-muted-foreground' />
          Clear
        </Button>
      </div>

      {/* Entry list. */}
      <div className='flex-1 overflow-y-auto font-mono text-2xs'>
        {filtered.length === 0 ? (
          search ? (
            <EmptyState
              size='compact'
              illustration={false}
              cause='filtered'
              title='No matching entries'
              reason={`Nothing matches "${search}".`}
              action={{ label: 'Clear filter', onClick: () => setSearch('') }}
            />
          ) : hiddenInfoCount > 0 ? (
            <EmptyState
              size='compact'
              illustration={false}
              cause='filtered'
              title='No warnings or errors'
              reason={`${hiddenInfoCount} INFO ${hiddenInfoCount === 1 ? 'log is' : 'logs are'} hidden.`}
              action={{ label: 'Show INFO logs', onClick: () => setShowInfoLogs(true) }}
            />
          ) : (
            <ConsoleEmptyState />
          )
        ) : (
          filtered.map((entry) => {
            if (entry.kind === 'log') {
              return <LogRow key={entry.id} entry={entry} />;
            }
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
                  onClick={() => toggleExpanded(entry.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      toggleExpanded(entry.id);
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
                  {entry.requestName && (
                    <span
                      data-testid='console-request-name'
                      className='max-w-48 truncate text-muted-foreground shrink-0'
                    >
                      {entry.requestName}
                    </span>
                  )}
                  <span className='font-semibold w-12 shrink-0'>{entry.method}</span>
                  <span className='flex-1 truncate text-foreground/80'>
                    {selectHttpView(entry, entry.id === revealedId).url}
                  </span>
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
                {expandedId === entry.id && (
                  <HttpEntryDetail
                    entry={entry}
                    revealed={revealedId === entry.id}
                    onToggleReveal={() =>
                      setRevealedId((current) => (current === entry.id ? null : entry.id))
                    }
                  />
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
