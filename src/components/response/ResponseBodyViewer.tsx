import {
  Check,
  Clock,
  Copy,
  FileText,
  Hash,
  LayoutPanelLeft,
  Search,
  Wifi,
  WifiOff,
  Zap,
} from 'lucide-react';
import { lazy, Suspense, useCallback, useMemo, useRef, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { statusBadgeColor, timeColor } from '@/lib/colors';
import type { ResponseState } from '@/types/pane-types';
import { ResponseHeadersTable } from './ResponseHeadersTable';

type ViewTab = ResponseState['activeView'];

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({
    default: m.MonacoWrapper,
  })),
);

interface ResponseBodyViewerProps {
  response: ResponseState;
}

function formatBody(body: string): { formatted: string; isJson: boolean } {
  try {
    const parsed = JSON.parse(body);
    return { formatted: JSON.stringify(parsed, null, 2), isJson: true };
  } catch {
    return { formatted: body, isJson: false };
  }
}

function formatXml(xml: string): string {
  try {
    const parser = new DOMParser();
    const doc = parser.parseFromString(xml, 'application/xml');
    if (doc.querySelector('parsererror')) return xml;
    const serialised = new XMLSerializer().serializeToString(doc);
    return serialised
      .replace(/></g, '>\n<')
      .split('\n')
      .map((line) => line.trim())
      .join('\n');
  } catch {
    return xml;
  }
}

function getContentType(headers: { key: string; value: string }[]): string {
  return headers.find((h) => h.key.toLowerCase() === 'content-type')?.value.toLowerCase() ?? '';
}

function detectResponseLanguage(contentType: string, isJson: boolean, isXml: boolean): string {
  if (isJson || contentType.includes('json')) return 'json';
  if (isXml || contentType.includes('xml')) return 'xml';
  if (contentType.includes('html')) return 'html';
  if (contentType.includes('javascript')) return 'javascript';
  if (contentType.includes('css')) return 'css';
  return 'plaintext';
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

// TTFB latency tier label shown in the status bar.
function ttfbLabel(ms: number): string {
  if (ms <= 100) return 'Fast';
  if (ms <= 500) return 'Ok';
  return 'Slow';
}

// Counts JSON keys at the root level for the summary chip.
function countJsonKeys(body: string): number | null {
  try {
    const parsed = JSON.parse(body);
    if (Array.isArray(parsed)) return parsed.length;
    if (parsed && typeof parsed === 'object') return Object.keys(parsed).length;
    return null;
  } catch {
    return null;
  }
}

// Tab button used in the custom tab bar.
function TabButton({
  active,
  onClick,
  children,
  role,
  ariaSelected,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
  role?: string;
  ariaSelected?: boolean;
}) {
  return (
    // biome-ignore lint/a11y/useAriaPropsSupportedByRole: aria-selected is valid when role="tab" is set
    <button
      type='button'
      role={role}
      aria-selected={ariaSelected}
      onClick={onClick}
      className={[
        'relative px-3 py-1.5 text-xs font-medium transition-colors select-none',
        active
          ? 'text-foreground after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-primary after:rounded-t-full'
          : 'text-muted-foreground hover:text-foreground',
      ].join(' ')}
    >
      {children}
    </button>
  );
}

// Inline search bar that appears when the user clicks the search icon.
function BodySearchBar({
  value,
  onChange,
  onClose,
}: {
  value: string;
  onChange: (v: string) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  return (
    <div className='flex items-center gap-1 border-b border-border/70 px-2 py-1 shrink-0 bg-muted/30'>
      <Search className='h-3 w-3 text-muted-foreground shrink-0' />
      <Input
        ref={ref}
        autoFocus
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder='Search in body…'
        className='h-5 border-0 bg-transparent p-0 text-xs shadow-none focus-visible:ring-0'
        onKeyDown={(e) => e.key === 'Escape' && onClose()}
      />
      <button
        type='button'
        onClick={onClose}
        className='text-xs text-muted-foreground hover:text-foreground transition-colors'
        aria-label='Close search'
      >
        <span aria-hidden='true'>×</span>
      </button>
    </div>
  );
}

// Empty state shown when there is no response body.
function EmptyBody({ label }: { label: string }) {
  return (
    <div className='flex h-full flex-col items-center justify-center gap-2 text-muted-foreground'>
      <WifiOff className='h-8 w-8 opacity-20' />
      <span className='text-xs'>{label}</span>
    </div>
  );
}

export function ResponseBodyViewer({ response }: ResponseBodyViewerProps) {
  const [activeView, setActiveView] = useState<ViewTab>(response.activeView ?? 'pretty');
  const [copied, setCopied] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

  const contentType = getContentType(response.headers);
  const { formatted, isJson } = useMemo(() => formatBody(response.body), [response.body]);
  const isXml =
    contentType.includes('xml') || (!isJson && response.body.trimStart().startsWith('<'));

  const prettyBody = useMemo(() => {
    if (isJson) return formatted;
    if (isXml) return formatXml(response.body);
    return response.body;
  }, [formatted, isJson, isXml, response.body]);

  const language = detectResponseLanguage(contentType, isJson, isXml);
  const jsonKeyCount = useMemo(
    () => (isJson ? countJsonKeys(response.body) : null),
    [isJson, response.body],
  );
  const headerCount = response.headers.length;

  // Filtered raw body for the search overlay.
  const filteredLines = useMemo(() => {
    if (!searchQuery || activeView !== 'raw') return null;
    const q = searchQuery.toLowerCase();
    return response.body
      .split('\n')
      .map((line, i) => ({ line, i: i + 1, match: line.toLowerCase().includes(q) }))
      .filter((l) => l.match);
  }, [searchQuery, activeView, response.body]);

  const handleCopyBody = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(prettyBody);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard API unavailable.
    }
  }, [prettyBody]);

  const handleSearchClose = () => {
    setSearchOpen(false);
    setSearchQuery('');
  };

  const isBodyTab = activeView === 'pretty' || activeView === 'raw';
  const hasBody = Boolean(response.body);

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      {/* ── Status bar ── */}
      <div className='flex items-center gap-2 border-b border-border/70 px-3 py-1.5 shrink-0 bg-card'>
        <span
          className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-semibold tracking-wide ${statusBadgeColor(response.status)}`}
        >
          {response.status === 0 ? 'ERR' : response.status}{' '}
          <span className='ml-1 font-normal opacity-80'>{response.statusText}</span>
        </span>

        {response.durationMs > 0 && (
          <span
            className={`inline-flex items-center gap-1 text-xs font-medium ${timeColor(response.durationMs)}`}
          >
            <Clock className='h-3 w-3' />
            {formatDuration(response.durationMs)}
          </span>
        )}

        {response.ttfbMs > 0 && (
          <span
            title={`Time to first byte: ${response.ttfbMs} ms`}
            className='inline-flex items-center gap-1 text-xs text-muted-foreground'
          >
            <Zap className='h-3 w-3' />
            TTFB {ttfbLabel(response.ttfbMs)}
          </span>
        )}

        <span className='inline-flex items-center gap-1 text-xs text-muted-foreground'>
          <FileText className='h-3 w-3' />
          {formatBytes(response.sizeBytes)}
        </span>

        {/* JSON key/array count chip. */}
        {jsonKeyCount !== null && (
          <span className='inline-flex items-center gap-1 text-xs text-muted-foreground'>
            <Hash className='h-3 w-3' />
            {jsonKeyCount} {Array.isArray(JSON.parse(response.body)) ? 'items' : 'keys'}
          </span>
        )}

        {/* Content-type chip when recognisable. */}
        {contentType && (
          <span className='ml-auto inline-flex items-center gap-1 rounded bg-muted/60 px-1.5 py-0.5 text-2xs font-mono text-muted-foreground truncate max-w-40'>
            <Wifi className='h-2.5 w-2.5 shrink-0' />
            {contentType.split(';')[0]}
          </span>
        )}
      </div>

      {/* ── Tab bar ── */}
      <div className='flex items-center border-b border-border/70 px-1 shrink-0 bg-card'>
        <div className='flex flex-1 items-center' role='tablist'>
          <TabButton
            active={activeView === 'pretty'}
            onClick={() => setActiveView('pretty')}
            role='tab'
            ariaSelected={activeView === 'pretty'}
          >
            Pretty
          </TabButton>
          <TabButton
            active={activeView === 'raw'}
            onClick={() => setActiveView('raw')}
            role='tab'
            ariaSelected={activeView === 'raw'}
          >
            Raw
          </TabButton>
          <TabButton
            active={activeView === 'preview'}
            onClick={() => setActiveView('preview')}
            role='tab'
            ariaSelected={activeView === 'preview'}
          >
            Preview
          </TabButton>
          <TabButton
            active={activeView === 'headers'}
            onClick={() => setActiveView('headers')}
            role='tab'
            ariaSelected={activeView === 'headers'}
          >
            Headers
            {headerCount > 0 && (
              <span className='ml-1 text-2xs text-muted-foreground'>({headerCount})</span>
            )}
          </TabButton>
        </div>

        {/* Toolbar actions for body tabs. */}
        {isBodyTab && hasBody && (
          <div className='flex items-center gap-0.5 pr-1'>
            {/* Toggle search (raw tab only — Monaco pretty has its own Ctrl+F). */}
            {activeView === 'raw' && (
              <Button
                variant='ghost'
                size='icon'
                className='h-6 w-6'
                title='Search in body'
                aria-label='Search in body'
                onClick={() => setSearchOpen((o) => !o)}
              >
                <Search className='h-3.5 w-3.5' />
              </Button>
            )}

            {/* Word-wrap / layout toggle placeholder — could wire to MonacoWrapper. */}
            {activeView === 'pretty' && (
              <Button
                variant='ghost'
                size='icon'
                className='h-6 w-6 text-muted-foreground'
                title='Toggle word wrap'
                aria-label='Toggle word wrap'
                disabled
              >
                <LayoutPanelLeft className='h-3.5 w-3.5' />
              </Button>
            )}

            <Button
              variant='ghost'
              size='icon'
              className='h-6 w-6'
              title='Copy response body'
              aria-label='Copy response body'
              onClick={handleCopyBody}
            >
              {copied ? (
                <Check className='h-3.5 w-3.5 text-emerald-500' />
              ) : (
                <Copy className='h-3.5 w-3.5' />
              )}
            </Button>
          </div>
        )}
      </div>

      {/* ── Optional search bar (raw tab) ── */}
      {searchOpen && activeView === 'raw' && (
        <BodySearchBar value={searchQuery} onChange={setSearchQuery} onClose={handleSearchClose} />
      )}

      {/* ── Tab content ── */}
      <div className='flex-1 min-h-0 overflow-hidden'>
        {/* Pretty tab. */}
        {activeView === 'pretty' &&
          (hasBody ? (
            <Suspense fallback={<EditorSkeleton />}>
              <MonacoWrapper value={prettyBody} language={language} readOnly height='100%' />
            </Suspense>
          ) : (
            <EmptyBody label='No response body' />
          ))}

        {/* Raw tab — with optional line-level search overlay. */}
        {activeView === 'raw' &&
          (hasBody ? (
            searchQuery && filteredLines ? (
              <div className='h-full overflow-auto p-3 font-mono text-xs'>
                {filteredLines.length === 0 ? (
                  <p className='text-muted-foreground'>No matches for "{searchQuery}"</p>
                ) : (
                  filteredLines.map(({ line, i }) => (
                    <div key={i} className='flex gap-3 leading-5'>
                      <span className='select-none w-8 text-right shrink-0 text-muted-foreground/50'>
                        {i}
                      </span>
                      <span className='break-all text-foreground'>{line}</span>
                    </div>
                  ))
                )}
              </div>
            ) : (
              <Suspense fallback={<EditorSkeleton />}>
                <MonacoWrapper value={response.body} language='plaintext' readOnly height='100%' />
              </Suspense>
            )
          ) : (
            <EmptyBody label='No response body' />
          ))}

        {/* Preview tab — sandboxed iframe. */}
        {activeView === 'preview' && (
          <div className='h-full overflow-auto p-3'>
            {/*
             * Security: sandbox="" blocks all permissions (scripts, forms, popups,
             * same-origin access). Do NOT add any sandbox tokens here.
             */}
            {hasBody ? (
              <iframe
                srcDoc={response.body}
                sandbox=''
                className='h-full min-h-48 w-full border-0 bg-white rounded'
                title='Response preview'
              />
            ) : (
              <EmptyBody label='No response body to preview' />
            )}
          </div>
        )}

        {/* Headers tab. */}
        {activeView === 'headers' && (
          <div className='h-full p-3 overflow-auto'>
            <ResponseHeadersTable headers={response.headers} />
          </div>
        )}
      </div>
    </div>
  );
}
