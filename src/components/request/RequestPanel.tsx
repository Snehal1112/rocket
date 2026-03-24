import { useState, useCallback, useRef, useEffect } from 'react';
import { Send } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import { parseQueryParams, buildUrl, splitUrl } from '@/lib/url-params';
import {
  executeRequest,
  type Auth,
  type Body,
  type Header,
} from '@/lib/tauri-api';
import { QueryParamsEditor } from './QueryParamsEditor';
import { PathParamsPanel } from './PathParamsPanel';
import { HeadersEditor } from './HeadersEditor';
import { BodyEditor } from './BodyEditor';
import { AuthEditor } from './AuthEditor';
import { ResponseBodyViewer } from '@/components/response/ResponseBodyViewer';
import type {
  Tab,
  HttpMethod,
  KeyValueEntry,
  AuthState,
  ResponseState,
} from '@/types/pane-types';

const METHODS: HttpMethod[] = [
  'GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD',
];

const METHOD_COLORS: Record<HttpMethod, string> = {
  GET: 'text-green-600 dark:text-green-400',
  POST: 'text-blue-600 dark:text-blue-400',
  PUT: 'text-orange-600 dark:text-orange-400',
  PATCH: 'text-yellow-600 dark:text-yellow-400',
  DELETE: 'text-red-600 dark:text-red-400',
  OPTIONS: 'text-gray-500',
  HEAD: 'text-gray-400',
};

type SectionTab = 'params' | 'headers' | 'body' | 'auth';

interface RequestPanelProps {
  tab: Tab;
  groupId: string;
}

function toApiAuth(auth: AuthState): Auth {
  switch (auth.authType) {
    case 'basic':
      return {
        authType: 'basic',
        username: auth.basic?.username ?? '',
        password: auth.basic?.password ?? '',
      };
    case 'bearer':
      return { authType: 'bearer', token: auth.bearer?.token ?? '' };
    case 'api-key':
      return {
        authType: 'api-key',
        key: auth.apiKey?.key ?? '',
        value: auth.apiKey?.value ?? '',
        addTo: auth.apiKey?.addTo ?? 'header',
      };
    default:
      return { authType: 'none' };
  }
}

function toApiBody(body: { mode: string; content: string; formData: KeyValueEntry[] }): Body | undefined {
  if (body.mode === 'none') return undefined;
  if (body.mode === 'formdata') {
    return {
      mode: 'formdata',
      formData: body.formData
        .filter((e) => e.enabled)
        .map((e) => ({
          key: e.key,
          value: e.value,
          entryType: 'text' as const,
          enabled: e.enabled,
        })),
    };
  }
  return { mode: body.mode as Body['mode'], content: body.content };
}

function statusColor(status: number): string {
  if (status >= 500) return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
  if (status >= 400) return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
  if (status >= 300) return 'bg-yellow-100 text-yellow-700 border-yellow-200 dark:bg-yellow-900/30 dark:text-yellow-400 dark:border-yellow-800';
  if (status >= 200) return 'bg-green-100 text-green-700 border-green-200 dark:bg-green-900/30 dark:text-green-400 dark:border-green-800';
  return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function RequestPanel({ tab, groupId: _groupId }: RequestPanelProps) {
  const { request, response } = tab;
  const updateRequest = usePaneStore((s) => s.updateRequest);

  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [sending, setSending] = useState(false);
  const [unsavedDialogOpen, setUnsavedDialogOpen] = useState(false);

  // Resizable split: request height as percentage.
  const containerRef = useRef<HTMLDivElement>(null);
  const [requestHeight, setRequestHeight] = useState(55);
  const [isDragging, setIsDragging] = useState(false);

  const urlSyncTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (urlSyncTimer.current) clearTimeout(urlSyncTimer.current);
    };
  }, []);

  // Drag handle for request/response split.
  const handleSeparatorDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      setIsDragging(true);
      const container = containerRef.current;
      if (!container) return;

      const startY = e.clientY;
      const startHeight = requestHeight;
      const containerH = container.getBoundingClientRect().height;

      const onMove = (ev: PointerEvent) => {
        const delta = ev.clientY - startY;
        const pct = startHeight + (delta / containerH) * 100;
        setRequestHeight(Math.min(80, Math.max(20, pct)));
      };
      const onUp = () => {
        setIsDragging(false);
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    },
    [requestHeight],
  );

  const handleUrlChange = useCallback(
    (url: string) => {
      updateRequest(tab.id, { url });
      if (urlSyncTimer.current) clearTimeout(urlSyncTimer.current);
      urlSyncTimer.current = setTimeout(() => {
        const parsed = parseQueryParams(url);
        updateRequest(tab.id, { queryParams: parsed });
      }, 300);
    },
    [tab.id, updateRequest],
  );

  const handleParamsChange = useCallback(
    (params: KeyValueEntry[]) => {
      const { base } = splitUrl(request.url);
      const newUrl = buildUrl(base, params);
      updateRequest(tab.id, { url: newUrl, queryParams: params });
    },
    [tab.id, request.url, updateRequest],
  );

  const handlePathUrlChange = useCallback(
    (newUrl: string) => {
      updateRequest(tab.id, { url: newUrl });
    },
    [tab.id, updateRequest],
  );

  const handleSend = useCallback(async () => {
    setSending(true);
    try {
      const headers: Header[] = request.headers
        .filter((h) => h.enabled)
        .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled }));

      const result = await executeRequest({
        method: request.method,
        url: request.url,
        headers,
        body: toApiBody(request.body),
        auth: toApiAuth(request.auth),
        options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
      });

      const responseState: ResponseState = {
        status: result.status,
        statusText: result.statusText,
        headers: result.headers.map((h) => ({
          id: crypto.randomUUID(),
          key: h.key,
          value: h.value,
          enabled: h.enabled,
        })),
        body: result.body,
        durationMs: result.durationMs,
        sizeBytes: result.sizeBytes,
        activeView: 'pretty',
      };
      usePaneStore.getState().setResponse(tab.id, responseState);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      usePaneStore.getState().setResponse(tab.id, {
        status: 0,
        statusText: 'Error',
        headers: [],
        body: msg,
        durationMs: 0,
        sizeBytes: msg.length,
        activeView: 'raw',
      });
    } finally {
      setSending(false);
    }
  }, [tab.id, request]);

  const enabledParamCount = request.queryParams.filter((p) => p.enabled).length;
  const enabledHeaderCount = request.headers.filter((h) => h.enabled).length;

  return (
    <div ref={containerRef} className="flex h-full flex-col overflow-hidden bg-transparent">
      {/* ── Request area ── */}
      <div
        className="flex flex-col overflow-hidden bg-card/80"
        style={{ height: `${requestHeight}%`, minHeight: '20%', maxHeight: '80%' }}
      >
        {/* URL bar. */}
        <div className="flex items-center gap-2 border-b border-border/70 px-3 py-2 bg-card/70 backdrop-blur-sm">
          <Select
            value={request.method}
            onValueChange={(val) => updateRequest(tab.id, { method: val as HttpMethod })}
          >
            <SelectTrigger
              className={cn('h-8 w-[7rem] text-xs font-bold', METHOD_COLORS[request.method])}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {METHODS.map((m) => (
                <SelectItem key={m} value={m} className={cn('text-xs font-bold', METHOD_COLORS[m])}>
                  {m}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Input
            className="h-8 flex-1 font-mono text-xs"
            placeholder="https://api.example.com/resource"
            value={request.url}
            onChange={(e) => handleUrlChange(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleSend(); }}
          />

          <Button size="sm" className="h-8 px-3" disabled={sending} onClick={handleSend}>
            <Send className="mr-1 h-3.5 w-3.5" />
            {sending ? 'Sending...' : 'Send'}
          </Button>
        </div>

        {/* Section tabs — matching legacy TabsList styling. */}
        <Tabs
          value={activeSection}
          onValueChange={(val) => setActiveSection(val as SectionTab)}
          className="flex-1 flex flex-col min-h-0"
        >
          <TabsList className="w-full justify-start rounded-none border-b border-border/70 bg-card/60 h-9 px-3">
            <TabsTrigger value="params" className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
              Params
              {enabledParamCount > 0 && (
                <span className="ml-1 text-[10px] text-muted-foreground">
                  ({enabledParamCount})
                </span>
              )}
            </TabsTrigger>
            <TabsTrigger value="headers" className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
              Headers
              {enabledHeaderCount > 0 && (
                <span className="ml-1 text-[10px] text-muted-foreground">
                  ({enabledHeaderCount})
                </span>
              )}
            </TabsTrigger>
            <TabsTrigger value="body" className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
              Body
            </TabsTrigger>
            <TabsTrigger value="auth" className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
              Auth
            </TabsTrigger>
          </TabsList>

          <div className="flex-1 overflow-auto p-3">
            <TabsContent value="params" className="mt-0 h-full">
              <div className="space-y-4">
                <PathParamsPanel url={request.url} onUrlChange={handlePathUrlChange} />
                <QueryParamsEditor params={request.queryParams} onChange={handleParamsChange} />
              </div>
            </TabsContent>
            <TabsContent value="headers" className="mt-0 h-full">
              <HeadersEditor
                headers={request.headers}
                onChange={(headers) => updateRequest(tab.id, { headers })}
              />
            </TabsContent>
            <TabsContent value="body" className="mt-0 h-full">
              <BodyEditor
                body={request.body}
                onChange={(body) => updateRequest(tab.id, { body })}
              />
            </TabsContent>
            <TabsContent value="auth" className="mt-0 h-full">
              <AuthEditor
                auth={request.auth}
                onChange={(auth) => updateRequest(tab.id, { auth })}
              />
            </TabsContent>
          </div>
        </Tabs>
      </div>

      {/* ── Drag separator ── */}
      <div
        onPointerDown={handleSeparatorDown}
        className={cn(
          'h-3 flex items-center justify-center cursor-row-resize select-none border-y transition-colors',
          isDragging
            ? 'bg-primary/15 border-primary/50'
            : 'bg-muted/50 border-border/70 hover:bg-accent/70 hover:border-primary/40',
        )}
      >
        <div
          className={cn(
            'rounded-full transition-all',
            isDragging ? 'w-24 h-1.5 bg-primary' : 'w-16 h-1 bg-muted-foreground/40',
          )}
        />
      </div>

      {/* ── Response area ── */}
      <div className="flex-1 flex flex-col overflow-hidden bg-card/65 min-h-0">
        {response ? (
          <>
            {/* Response status bar. */}
            <div className="flex items-center gap-3 border-b border-border/70 px-3 py-1.5 text-xs bg-card/50 shrink-0">
              <Badge variant="outline" className={cn('text-xs font-semibold', statusColor(response.status))}>
                {response.status === 0 ? 'ERR' : response.status} {response.statusText}
              </Badge>
              {response.durationMs > 0 && (
                <span className="text-muted-foreground">{response.durationMs} ms</span>
              )}
              <span className="text-muted-foreground">{formatBytes(response.sizeBytes)}</span>
            </div>

            {/* Response body viewer. */}
            <div className="flex-1 overflow-hidden">
              <ResponseBodyViewer response={response} />
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
            Send a request to see the response
          </div>
        )}
      </div>

      {/* Unsaved changes dialog. */}
      <AlertDialog open={unsavedDialogOpen} onOpenChange={setUnsavedDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Unsaved Changes</AlertDialogTitle>
            <AlertDialogDescription>
              This tab has unsaved changes. Do you want to discard them?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => setUnsavedDialogOpen(false)}>
              Discard
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
