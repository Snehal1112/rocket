import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { Send, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { RocketLiftOff } from '@/components/illustrations';
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
import { parseQueryParams, buildUrl, splitUrl, extractPathParams } from '@/lib/url-params';
import { useExecuteRequest } from '@/hooks/useExecuteRequest';
import { QueryParamsEditor } from './QueryParamsEditor';
import { PathParamsPanel } from './PathParamsPanel';
import { HeadersEditor } from './HeadersEditor';
import { BodyEditor } from './BodyEditor';
import { AuthEditor } from './AuthEditor';
import { ResponseBodyViewer } from '@/components/response/ResponseBodyViewer';
import { SaveRequestButton } from './SaveRequestButton';
import { VariableAwareUrlInput } from './VariableAwareUrlInput';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import type {
  RequestTab,
  HttpMethod,
  KeyValueEntry,
} from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';
import { findTabInTree } from '@/lib/pane-utils';
import type { ParsedCurl } from '@/lib/curl-parser';
import { getCollectionSettings } from '@/lib/tauri-api';

const METHODS: HttpMethod[] = [
  'GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD',
];

type SectionTab = 'params' | 'headers' | 'body' | 'auth';

interface RequestPanelProps {
  tab: RequestTab;
  groupId: string;
}


export function RequestPanel({ tab, groupId: _groupId }: RequestPanelProps) {
  const { request, response } = tab;
  const updateRequest = usePaneStore((s) => s.updateRequest);

  const { send, sending } = useExecuteRequest(tab.id);

  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [unsavedDialogOpen, setUnsavedDialogOpen] = useState(false);
  const [urlError, setUrlError] = useState('');
  const [collectionVars, setCollectionVars] = useState<Record<string, string>>({});
  const [curlImported, setCurlImported] = useState(false);

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

  // Fetch collection variables for URL input overlay.
  useEffect(() => {
    if (!tab.source?.collection) { setCollectionVars({}); return; }
    getCollectionSettings(tab.source.collection)
      .then((s) => {
        const vars: Record<string, string> = {};
        for (const v of s.variables) {
          if (v.enabled) vars[v.key] = v.value || v.initialValue;
        }
        setCollectionVars(vars);
      })
      .catch(() => setCollectionVars({}));
  }, [tab.source?.collection]);

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

        // Auto-extract :pathParam names from URL and sync with pathParams state.
        const paramNames = extractPathParams(url);
        const existingParams = usePaneStore.getState().root;
        const found = findTabInTree(existingParams, tab.id);
        const currentPathParams = found && isRequestTab(found.tab) ? found.tab.request.pathParams : [];

        const newPathParams = paramNames.map((name) => {
          const existing = currentPathParams.find((p) => p.key === name);
          if (existing) return existing;
          return { id: crypto.randomUUID(), key: name, value: '', enabled: true };
        });

        updateRequest(tab.id, { queryParams: parsed, pathParams: newPathParams });
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

  const handleCurlImport = useCallback((parsed: ParsedCurl) => {
    const patch: Partial<typeof request> = {
      method: (parsed.method as HttpMethod) || 'GET',
      url: parsed.url,
      headers: parsed.headers.map((h) => ({
        id: crypto.randomUUID(),
        key: h.key,
        value: h.value,
        enabled: true,
      })),
    };

    if (parsed.body) {
      patch.body = {
        mode: parsed.body.mode,
        content: parsed.body.content,
        formData: [],
      };
    }

    if (parsed.auth?.type === 'basic') {
      patch.auth = {
        authType: 'basic',
        basic: { username: parsed.auth.username, password: parsed.auth.password },
      };
    }

    // Sync query params from the parsed URL.
    patch.queryParams = parseQueryParams(parsed.url);

    updateRequest(tab.id, patch);
    setUrlError('');
    setCurlImported(true);
    setTimeout(() => setCurlImported(false), 3000);

    // Auto-switch to the most relevant tab for the imported data.
    if (parsed.body) {
      setActiveSection('body');
    } else if (parsed.auth) {
      setActiveSection('auth');
    } else if (parsed.headers.length > 0) {
      setActiveSection('headers');
    }
  }, [tab.id, updateRequest]);

  const enabledParamCount = request.queryParams.filter((p) => p.enabled).length;
  const enabledHeaderCount = request.headers.filter((h) => h.enabled).length;

  const pathParamMap = useMemo(() => {
    const map: Record<string, string> = {};
    for (const p of request.pathParams) {
      if (p.enabled && p.key) map[p.key] = p.value;
    }
    return map;
  }, [request.pathParams]);

  const queryParamMap = useMemo(() => {
    const map: Record<string, string> = {};
    for (const p of request.queryParams) {
      if (p.enabled && p.key) map[p.key] = p.value;
    }
    return map;
  }, [request.queryParams]);

  return (
    <div ref={containerRef} className="flex h-full flex-col overflow-hidden bg-transparent">
      {/* ── Request area ── */}
      <div
        className="flex flex-col overflow-hidden bg-card/80 h-[var(--req-h)] min-h-[20%] max-h-[80%]"
        style={{ '--req-h': `${requestHeight}%` } as React.CSSProperties}
      >
        {/* URL bar. */}
        <div className="flex items-center gap-2 border-b border-border/70 px-3 py-2 bg-card/70 backdrop-blur-sm">
          <Select
            value={request.method}
            onValueChange={(val) => updateRequest(tab.id, { method: val as HttpMethod })}
          >
            <SelectTrigger
              className={cn('h-8 w-[7rem] text-sm font-semibold', METHOD_TEXT_COLOR[request.method])}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {METHODS.map((m) => (
                <SelectItem key={m} value={m} className={cn('text-sm font-semibold', METHOD_TEXT_COLOR[m])}>
                  {m}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => { setUrlError(''); handleUrlChange(val); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
            onCurlImport={handleCurlImport}
            collectionVariables={collectionVars}
            pathParams={pathParamMap}
            queryParams={queryParamMap}
            onPathParamChange={(key, val) => {
              const updated = request.pathParams.map((p) =>
                p.key === key ? { ...p, value: val } : p,
              );
              updateRequest(tab.id, { pathParams: updated });
            }}
            onSwitchToParams={() => setActiveSection('params')}
            placeholder="https://api.example.com/resource"
          />

          <Button size="sm" className="h-8 px-3" disabled={sending} onClick={() => {
            const url = request.url.trim();
            if (!url) { setUrlError('URL is required'); return; }
            try { new URL(url); } catch { setUrlError('Invalid URL — include http:// or https://'); return; }
            setUrlError('');
            send(request);
          }}>
            <Send className="mr-1 h-3.5 w-3.5" />
            {sending ? 'Sending...' : 'Send'}
          </Button>

          <SaveRequestButton tab={tab} groupId={_groupId} />
        </div>
        {urlError && (
          <p className="text-xs text-destructive px-3 py-1">{urlError}</p>
        )}
        {curlImported && (
          <p className="text-xs text-green-600 dark:text-green-400 px-3 py-1">Imported from cURL</p>
        )}

        {/* Section tabs — matching legacy TabsList styling. */}
        <Tabs
          value={activeSection}
          onValueChange={(val) => setActiveSection(val as SectionTab)}
          className="flex-1 flex flex-col min-h-0"
        >
          <TabsList>
            <TabsTrigger value="params">
              Params
              {enabledParamCount > 0 && (
                <span className="ml-1 text-2xs text-muted-foreground">
                  ({enabledParamCount})
                </span>
              )}
            </TabsTrigger>
            <TabsTrigger value="headers">
              Headers
              {enabledHeaderCount > 0 && (
                <span className="ml-1 text-2xs text-muted-foreground">
                  ({enabledHeaderCount})
                </span>
              )}
            </TabsTrigger>
            <TabsTrigger value="body">Body</TabsTrigger>
            <TabsTrigger value="auth">Auth</TabsTrigger>
          </TabsList>

          <div className="flex-1 overflow-auto p-3">
            <TabsContent value="params" className="mt-0 h-full">
              <div className="space-y-2">
                <PathParamsPanel
                  params={request.pathParams}
                  onChange={(pathParams) => updateRequest(tab.id, { pathParams })}
                />
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
                showInherit={!!tab.source}
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
        {sending ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
            <p className="text-sm text-muted-foreground">Sending request...</p>
          </div>
        ) : response ? (
          <ResponseBodyViewer response={response} />
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <RocketLiftOff className="w-24 h-24" />
            <p className="text-sm font-medium text-foreground">Ready for liftoff</p>
            <p className="text-xs text-muted-foreground">
              Send a request to see the response here
            </p>
            <p className="text-xs text-muted-foreground mt-1">
              Press{' '}
              <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-2xs">
                Ctrl+Enter
              </kbd>
              {' '}to send
            </p>
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
