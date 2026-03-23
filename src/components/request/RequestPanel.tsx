import { useState, useCallback, useRef, useEffect } from 'react';
import { Send, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
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
import type {
  Tab,
  HttpMethod,
  KeyValueEntry,
  AuthState,
  ResponseState,
} from '@/types/pane-types';

// HTTP methods and their corresponding badge colors.
const METHODS: HttpMethod[] = [
  'GET',
  'POST',
  'PUT',
  'PATCH',
  'DELETE',
  'OPTIONS',
  'HEAD',
];

const METHOD_COLORS: Record<HttpMethod, string> = {
  GET: 'text-emerald-500',
  POST: 'text-amber-500',
  PUT: 'text-blue-500',
  PATCH: 'text-violet-500',
  DELETE: 'text-red-500',
  OPTIONS: 'text-cyan-500',
  HEAD: 'text-gray-400',
};

type SectionTab = 'params' | 'headers' | 'body' | 'auth';

interface RequestPanelProps {
  tab: Tab;
  groupId: string;
}

// Maps pane-types AuthState to the tauri-api Auth tagged union.
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

// Maps pane-types BodyState to tauri-api Body.
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

// Status code color helper.
function statusColor(status: number): string {
  if (status < 300) return 'text-emerald-500';
  if (status < 400) return 'text-blue-500';
  if (status < 500) return 'text-amber-500';
  return 'text-red-500';
}

// Human-friendly byte size.
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function RequestPanel({ tab, groupId: _groupId }: RequestPanelProps) {
  const { request, response } = tab;
  const updateRequest = usePaneStore((s) => s.updateRequest);

  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [methodOpen, setMethodOpen] = useState(false);
  const [sending, setSending] = useState(false);

  // Debounce timer ref for URL-to-params sync.
  const urlSyncTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clean up debounce timer on unmount.
  useEffect(() => {
    return () => {
      if (urlSyncTimer.current) clearTimeout(urlSyncTimer.current);
    };
  }, []);

  // Handle URL input change with debounced param parsing.
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

  // Handle query param changes — immediately rebuild URL.
  const handleParamsChange = useCallback(
    (params: KeyValueEntry[]) => {
      const { base } = splitUrl(request.url);
      const newUrl = buildUrl(base, params);
      updateRequest(tab.id, { url: newUrl, queryParams: params });
    },
    [tab.id, request.url, updateRequest],
  );

  // Handle path param URL change.
  const handlePathUrlChange = useCallback(
    (newUrl: string) => {
      updateRequest(tab.id, { url: newUrl });
    },
    [tab.id, updateRequest],
  );

  // Send the request.
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
        options: {
          followRedirects: true,
          timeoutMs: 30000,
          verifySsl: true,
        },
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
        activeView: 'raw',
      };

      usePaneStore.getState().setResponse(tab.id, responseState);
    } catch (err) {
      // Store an error response so the user sees feedback.
      const errorMessage =
        err instanceof Error ? err.message : String(err);
      usePaneStore.getState().setResponse(tab.id, {
        status: 0,
        statusText: 'Error',
        headers: [],
        body: errorMessage,
        durationMs: 0,
        sizeBytes: errorMessage.length,
        activeView: 'raw',
      });
    } finally {
      setSending(false);
    }
  }, [tab.id, request]);

  const enabledParamCount = request.queryParams.filter((p) => p.enabled).length;
  const enabledHeaderCount = request.headers.filter((h) => h.enabled).length;

  return (
    <div className="flex h-full flex-col">
      {/* URL bar. */}
      <div className="flex items-center gap-2 border-b border-border p-2">
        {/* Method selector. */}
        <div className="relative">
          <button
            type="button"
            className={cn(
              'flex h-8 items-center gap-1 rounded-md border border-input px-2 text-xs font-bold',
              METHOD_COLORS[request.method],
            )}
            onClick={() => setMethodOpen((v) => !v)}
            aria-label="Select HTTP method"
            aria-expanded={methodOpen}
          >
            {request.method}
            <ChevronDown className="h-3 w-3" />
          </button>
          {methodOpen && (
            <div className="absolute left-0 top-full z-50 mt-1 min-w-[6rem] rounded-md border border-border bg-popover p-1 shadow-md">
              {METHODS.map((m) => (
                <button
                  key={m}
                  type="button"
                  className={cn(
                    'block w-full rounded px-2 py-1 text-left text-xs font-bold hover:bg-muted',
                    METHOD_COLORS[m],
                  )}
                  onClick={() => {
                    updateRequest(tab.id, { method: m });
                    setMethodOpen(false);
                  }}
                >
                  {m}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* URL input. */}
        <Input
          className="h-8 flex-1 font-mono text-xs"
          placeholder="https://api.example.com/resource"
          value={request.url}
          onChange={(e) => handleUrlChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') handleSend();
          }}
          aria-label="Request URL"
        />

        {/* Send button. */}
        <Button
          size="sm"
          className="h-8 px-3"
          disabled={sending}
          onClick={handleSend}
        >
          <Send className="mr-1 h-3.5 w-3.5" />
          {sending ? 'Sending...' : 'Send'}
        </Button>
      </div>

      {/* Section tabs. */}
      <div className="flex gap-1 border-b border-border px-2">
        <SectionTabButton
          label="Params"
          count={enabledParamCount}
          active={activeSection === 'params'}
          onClick={() => setActiveSection('params')}
        />
        <SectionTabButton
          label="Headers"
          count={enabledHeaderCount}
          active={activeSection === 'headers'}
          onClick={() => setActiveSection('headers')}
        />
        <SectionTabButton
          label="Body"
          active={activeSection === 'body'}
          onClick={() => setActiveSection('body')}
        />
        <SectionTabButton
          label="Auth"
          active={activeSection === 'auth'}
          onClick={() => setActiveSection('auth')}
        />
      </div>

      {/* Section content. */}
      <div className="flex-1 overflow-auto p-3">
        {activeSection === 'params' && (
          <div className="space-y-4">
            <PathParamsPanel
              url={request.url}
              onUrlChange={handlePathUrlChange}
            />
            <QueryParamsEditor
              params={request.queryParams}
              onChange={handleParamsChange}
            />
          </div>
        )}

        {activeSection === 'headers' && (
          <HeadersEditor
            headers={request.headers}
            onChange={(headers) => updateRequest(tab.id, { headers })}
          />
        )}

        {activeSection === 'body' && (
          <BodyEditor
            body={request.body}
            onChange={(body) => updateRequest(tab.id, { body })}
          />
        )}

        {activeSection === 'auth' && (
          <AuthEditor
            auth={request.auth}
            onChange={(auth) => updateRequest(tab.id, { auth })}
          />
        )}
      </div>

      {/* Response area — shown only when a response exists. */}
      {response && (
        <div className="border-t-2 border-border">
          {/* Response status bar. */}
          <div className="flex items-center gap-3 border-b border-border px-3 py-1.5 text-xs">
            <span className={cn('font-bold', statusColor(response.status))}>
              {response.status === 0 ? 'ERR' : response.status}{' '}
              {response.statusText}
            </span>
            {response.durationMs > 0 && (
              <span className="text-muted-foreground">
                {response.durationMs} ms
              </span>
            )}
            <span className="text-muted-foreground">
              {formatBytes(response.sizeBytes)}
            </span>
          </div>

          {/* Response body. */}
          <div className="max-h-64 overflow-auto p-3">
            <pre className="whitespace-pre-wrap break-all font-mono text-xs">
              {response.body}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

// Small helper component for section tab buttons.
function SectionTabButton({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count?: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={cn(
        'relative px-3 py-2 text-xs font-medium transition-colors',
        active
          ? 'text-foreground'
          : 'text-muted-foreground hover:text-foreground',
      )}
      onClick={onClick}
    >
      {label}
      {count != null && count > 0 && (
        <span className="ml-1 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-[10px] font-semibold">
          {count}
        </span>
      )}
      {active && (
        <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-primary" />
      )}
    </button>
  );
}
