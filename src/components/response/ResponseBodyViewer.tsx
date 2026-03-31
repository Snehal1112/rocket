import { lazy, Suspense, useMemo, useState } from 'react';
import { Copy, Check, Clock, FileText } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ResponseHeadersTable } from './ResponseHeadersTable';
import { statusBadgeColor, timeColor } from '@/lib/colors';
import type { ResponseState } from '@/types/pane-types';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';

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
          className={`inline-flex items-center rounded-md border px-2 py-0.5 text-sm font-semibold ${statusBadgeColor(response.status)}`}
        >
          {response.status === 0 ? 'ERR' : response.status} {response.statusText}
        </span>

        {/* Time badge with clock icon, color-coded. */}
        {response.durationMs > 0 && (
          <span className={`inline-flex items-center gap-1 text-xs font-medium ${timeColor(response.durationMs)}`}>
            <Clock className="h-3.5 w-3.5 text-muted-foreground" />
            {formatDuration(response.durationMs)}
          </span>
        )}

        {/* Size badge with file icon. */}
        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
          <FileText className="h-3.5 w-3.5 text-muted-foreground" />
          {formatBytes(response.sizeBytes)}
        </span>
      </div>

      {/* Tab bar with copy button. */}
      <div className="flex items-center border-b border-border/70 px-1 shrink-0">
        <Tabs
          value={activeView}
          onValueChange={(v) => setActiveView(v as ViewTab)}
          className="flex-1"
        >
          <TabsList>
            <TabsTrigger value="pretty">Pretty</TabsTrigger>
            <TabsTrigger value="raw">Raw</TabsTrigger>
            <TabsTrigger value="preview">Preview</TabsTrigger>
            <TabsTrigger value="headers">
              Headers
              {headerCount > 0 && (
                <span className="ml-1 text-2xs text-muted-foreground">
                  ({headerCount})
                </span>
              )}
            </TabsTrigger>
          </TabsList>
        </Tabs>

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
              <Check className="h-3.5 w-3.5 text-emerald-500" />
            ) : (
              <Copy className="h-3.5 w-3.5" />
            )}
          </Button>
        )}
      </div>

      {/* Tab content — fills remaining height. */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {activeView === 'pretty' && (
          response.body ? (
            <Suspense fallback={<EditorSkeleton />}>
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
            <Suspense fallback={<EditorSkeleton />}>
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
