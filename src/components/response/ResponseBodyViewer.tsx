import { lazy, Suspense, useMemo, useState } from 'react';
import { Copy, Check, Clock, FileText } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ResponseHeadersTable } from './ResponseHeadersTable';
import type { ResponseState } from '@/types/pane-types';

type ViewTab = ResponseState['activeView'];

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({
    default: m.MonacoWrapper,
  })),
);

interface ResponseBodyViewerProps {
  response: ResponseState;
}

// Try to detect and pretty-print JSON; fall back to raw body.
function formatBody(body: string): { formatted: string; isJson: boolean } {
  try {
    const parsed = JSON.parse(body);
    return { formatted: JSON.stringify(parsed, null, 2), isJson: true };
  } catch {
    return { formatted: body, isJson: false };
  }
}

// Attempt basic XML indentation via DOMParser.
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

// Detect content type from response headers.
function getContentType(headers: { key: string; value: string }[]): string {
  return (
    headers.find((h) => h.key.toLowerCase() === 'content-type')?.value.toLowerCase() ?? ''
  );
}

// Detect the Monaco language from the content type.
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

// Color-coded status badge based on HTTP status range.
function statusClasses(status: number): string {
  if (status >= 500) return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
  if (status >= 400) return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
  if (status >= 300) return 'bg-yellow-100 text-yellow-700 border-yellow-200 dark:bg-yellow-900/30 dark:text-yellow-400 dark:border-yellow-800';
  if (status >= 200) return 'bg-green-100 text-green-700 border-green-200 dark:bg-green-900/30 dark:text-green-400 dark:border-green-800';
  return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
}

// Color-coded time: green (<200ms), yellow (200-1000ms), red (>1s).
function timeClasses(ms: number): string {
  if (ms <= 200) return 'text-green-600 dark:text-green-400';
  if (ms <= 1000) return 'text-yellow-600 dark:text-yellow-400';
  return 'text-red-600 dark:text-red-400';
}

export function ResponseBodyViewer({ response }: ResponseBodyViewerProps) {
  const [activeView, setActiveView] = useState<ViewTab>(
    response.activeView ?? 'pretty',
  );
  const [copied, setCopied] = useState(false);

  const contentType = getContentType(response.headers);

  // Memoize the formatted body to avoid re-parsing on every render.
  const { formatted, isJson } = useMemo(
    () => formatBody(response.body),
    [response.body],
  );

  const isXml = contentType.includes('xml') || (!isJson && response.body.trimStart().startsWith('<'));

  const prettyBody = useMemo(() => {
    if (isJson) return formatted;
    if (isXml) return formatXml(response.body);
    return response.body;
  }, [formatted, isJson, isXml, response.body]);

  const language = detectResponseLanguage(contentType, isJson, isXml);

  const handleCopyBody = async () => {
    try {
      await navigator.clipboard.writeText(prettyBody);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard API not available.
    }
  };

  const headerCount = response.headers.length;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Status bar — status code, duration, size. */}
      <div className="flex items-center gap-2 border-b border-border/70 px-3 py-1.5 shrink-0">
        {/* Status badge. */}
        <span
          className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-semibold ${statusClasses(response.status)}`}
        >
          {response.status === 0 ? 'ERR' : response.status} {response.statusText}
        </span>

        {/* Time badge with clock icon, color-coded. */}
        {response.durationMs > 0 && (
          <span className={`inline-flex items-center gap-1 text-xs font-medium ${timeClasses(response.durationMs)}`}>
            <Clock className="h-3 w-3" />
            {formatDuration(response.durationMs)}
          </span>
        )}

        {/* Size badge with file icon. */}
        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
          <FileText className="h-3 w-3" />
          {formatBytes(response.sizeBytes)}
        </span>
      </div>

      {/* Tab bar with copy button. */}
      <div className="flex items-center border-b border-border/70 px-1 shrink-0">
        <div className="flex h-8 flex-1 items-center gap-0">
          {(['pretty', 'raw', 'preview', 'headers'] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveView(tab)}
              className={`h-7 px-3 text-xs capitalize transition-colors ${
                activeView === tab
                  ? 'border-b-2 border-primary text-foreground font-medium'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              {tab}
              {tab === 'headers' && headerCount > 0 && (
                <span className="ml-1 text-2xs text-muted-foreground">
                  ({headerCount})
                </span>
              )}
            </button>
          ))}
        </div>

        {/* Copy body button — visible on body tabs. */}
        {(activeView === 'pretty' || activeView === 'raw') && response.body && (
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 mr-1 shrink-0"
            title="Copy response body"
            onClick={handleCopyBody}
          >
            {copied ? (
              <Check className="h-3 w-3 text-emerald-500" />
            ) : (
              <Copy className="h-3 w-3" />
            )}
          </Button>
        )}
      </div>

      {/* Tab content — fills remaining height. */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {activeView === 'pretty' && (
          response.body ? (
            <Suspense
              fallback={
                <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                  Loading editor...
                </div>
              }
            >
              <MonacoWrapper
                value={prettyBody}
                language={language}
                readOnly
                height="100%"
              />
            </Suspense>
          ) : (
            <div className="flex items-center justify-center h-32 text-muted-foreground text-xs">
              No response body
            </div>
          )
        )}

        {activeView === 'raw' && (
          response.body ? (
            <Suspense
              fallback={
                <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                  Loading editor...
                </div>
              }
            >
              <MonacoWrapper
                value={response.body}
                language="plaintext"
                readOnly
                height="100%"
              />
            </Suspense>
          ) : (
            <div className="flex items-center justify-center h-32 text-muted-foreground text-xs">
              No response body
            </div>
          )
        )}

        {activeView === 'preview' && (
          <div className="h-full p-3 overflow-auto">
            {/*
             * Security: sandbox="" blocks all permissions (scripts, forms, popups,
             * same-origin access), preventing untrusted response content from
             * executing code. Do NOT add any sandbox tokens here.
             */}
            <iframe
              srcDoc={response.body}
              sandbox=""
              className="h-full min-h-48 w-full border-0 bg-white rounded"
              title="Response preview"
            />
          </div>
        )}

        {activeView === 'headers' && (
          <div className="h-full p-3 overflow-auto">
            <ResponseHeadersTable headers={response.headers} />
          </div>
        )}
      </div>
    </div>
  );
}
