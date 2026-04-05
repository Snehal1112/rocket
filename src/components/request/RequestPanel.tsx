import { Loader2, Send, Zap } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { RocketLiftOff } from '@/components/illustrations';
import { LoadTestDialog } from '@/components/request/LoadTestDialog';
import { ResponseBodyViewer } from '@/components/response/ResponseBodyViewer';
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
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useExecuteRequest } from '@/hooks/useExecuteRequest';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import type { ParsedCurl } from '@/lib/curl-parser';
import { findTabInTree } from '@/lib/pane-utils';
import { type CollectionVariable, getCollectionSettings, updateRequestDocs } from '@/lib/tauri-api';
import { buildUrl, extractPathParams, parseQueryParams, splitUrl } from '@/lib/url-params';
import { buildScopedContext } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type {
  AuthState,
  BodyState,
  HttpMethod,
  KeyValueEntry,
  RequestTab,
} from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';
import { AuthEditor } from './AuthEditor';
import { BodyEditor } from './BodyEditor';
import { BrunoTabBar } from './BrunoTabBar';
import { HeadersEditor } from './HeadersEditor';
import { PathParamsPanel } from './PathParamsPanel';
import { QueryParamsEditor } from './QueryParamsEditor';
import { RequestDocsPanel } from './RequestDocsPanel';
import { RequestVariablesPanel } from './RequestVariablesPanel';
import { SaveRequestButton } from './SaveRequestButton';
import { SaveToCollectionDialog } from './SaveToCollectionDialog';
import { VariableAwareUrlInput } from './VariableAwareUrlInput';

const METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD'];

const BODY_MODES: { label: string; value: BodyState['mode'] }[] = [
  { label: 'None', value: 'none' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Text', value: 'text' },
  { label: 'Form Data', value: 'formdata' },
  { label: 'Binary', value: 'binary' },
];

const BASE_AUTH_TYPES: { label: string; value: AuthState['authType'] }[] = [
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
];

const INHERIT_AUTH_OPTION = {
  label: 'Inherit from parent',
  value: 'inherit' as AuthState['authType'],
};

type SectionTab = 'params' | 'headers' | 'body' | 'auth' | 'variables' | 'docs';

interface RequestPanelProps {
  tab: RequestTab;
  groupId: string;
}

export function RequestPanel({ tab, groupId: _groupId }: RequestPanelProps) {
  const { request, response } = tab;
  const updateRequest = usePaneStore((s) => s.updateRequest);

  const { send, sending } = useExecuteRequest(tab.id);

  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  const [unsavedDialogOpen, setUnsavedDialogOpen] = useState(false);
  const [showLoadTest, setShowLoadTest] = useState(false);
  const [saveToCollectionOpen, setSaveToCollectionOpen] = useState(false);
  const [urlError, setUrlError] = useState('');
  const [collectionVars, setCollectionVars] = useState<Record<string, string>>({});
  const [collectionVariables, setCollectionVariables] = useState<CollectionVariable[]>([]);
  const [curlImported, setCurlImported] = useState(false);
  const [requestVarCount, setRequestVarCount] = useState(0);

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

  // Listen for save-to-collection events targeted at this tab.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId === tab.id) setSaveToCollectionOpen(true);
    };
    window.addEventListener('rocket:save-to-collection', handler);
    return () => window.removeEventListener('rocket:save-to-collection', handler);
  }, [tab.id]);

  // Fetch collection variables for URL input overlay.
  useEffect(() => {
    if (!tab.source?.collection) {
      setCollectionVars({});
      setCollectionVariables([]);
      return;
    }
    getCollectionSettings(tab.source.collection)
      .then((s) => {
        const vars: Record<string, string> = {};
        for (const v of s.variables) {
          if (v.enabled) vars[v.key] = v.value || v.initialValue;
        }
        setCollectionVars(vars);
        setCollectionVariables(s.variables);
      })
      .catch(() => {
        setCollectionVars({});
        setCollectionVariables([]);
      });
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
        const currentPathParams =
          found && isRequestTab(found.tab) ? found.tab.request.pathParams : [];

        const newPathParams = paramNames.map((name) => {
          const existing = currentPathParams.find((p) => p.key === name);
          if (existing) return existing;
          return {
            id: crypto.randomUUID(),
            key: name,
            value: '',
            enabled: true,
          };
        });

        updateRequest(tab.id, {
          queryParams: parsed,
          pathParams: newPathParams,
        });
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

  const handleCurlImport = useCallback(
    (parsed: ParsedCurl) => {
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
          basic: {
            username: parsed.auth.username,
            password: parsed.auth.password,
          },
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
    },
    [tab.id, updateRequest],
  );

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

  // Select stable state slices instead of calling getter methods outside a memo,
  // which would produce new object references on every render.
  const activeEnvIdForScope = useEnvStore((s) => s.activeEnvId);
  const environments = useEnvStore((s) => s.environments);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const processEnvVars = useEnvStore((s) => s.processEnvVars);

  // Build the scope-aware variable context for the URL input overlay.
  // folderVars and requestVars are intentionally omitted: the Tauri commands for
  // reading per-folder and per-request variables at render time are not yet
  // implemented (deferred per the variables spec). They will be wired in once
  // the corresponding backend commands exist.
  const scopedContext = useMemo(() => {
    const envVars: Record<string, string> = {};
    if (activeEnvIdForScope) {
      const env = environments.find((e) => e.name === activeEnvIdForScope);
      if (env) for (const v of env.variables) if (v.enabled) envVars[v.key] = v.value;
    }
    const globalVars: Record<string, string> = globalEnv
      ? Object.fromEntries(
          globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
        )
      : {};
    return buildScopedContext({
      envVars,
      envLabel: activeEnvIdForScope ?? undefined,
      globalVars,
      processEnvVars,
      collectionVars: collectionVariables,
    });
  }, [activeEnvIdForScope, environments, globalEnv, processEnvVars, collectionVariables]);

  const authTypeOptions = useMemo(
    () => (tab.source ? [INHERIT_AUTH_OPTION, ...BASE_AUTH_TYPES] : BASE_AUTH_TYPES),
    [tab.source],
  );

  const handleAuthTypeChange = useCallback(
    (authType: AuthState['authType']) => {
      const next: AuthState = { authType };
      if (authType === 'basic') next.basic = { username: '', password: '' };
      if (authType === 'bearer') next.bearer = { token: '' };
      if (authType === 'api-key') next.apiKey = { key: '', value: '', addTo: 'header' };
      if (authType === 'oauth2')
        next.oauth2 = {
          grantType: 'client_credentials',
          authorizationUrl: '',
          tokenUrl: '',
          callbackUrl: 'https://exchange4all.local/webapp/#oidc-callback',
          clientId: '',
          clientSecret: '',
          scope: '',
          state: '',
          username: '',
          password: '',
          clientAuthentication: 'body',
          headerPrefix: 'Bearer',
          addTokenTo: 'header',
          verifySsl: true,
          accessToken: '',
          refreshToken: '',
          expiresIn: null,
          tokenAcquiredAt: null,
        };
      if (authType === 'aws-sig-v4')
        next.awsSigV4 = {
          accessKey: '',
          secretKey: '',
          region: '',
          service: '',
          sessionToken: '',
        };
      updateRequest(tab.id, { auth: next });
    },
    [tab.id, updateRequest],
  );

  const handleHeadersChange = useCallback(
    (headers: KeyValueEntry[]) => updateRequest(tab.id, { headers }),
    [tab.id, updateRequest],
  );

  const handleBodyChange = useCallback(
    (body: BodyState) => updateRequest(tab.id, { body }),
    [tab.id, updateRequest],
  );

  const handleAuthChange = useCallback(
    (auth: AuthState) => updateRequest(tab.id, { auth }),
    [tab.id, updateRequest],
  );

  const handlePathParamsChange = useCallback(
    (pathParams: KeyValueEntry[]) => updateRequest(tab.id, { pathParams }),
    [tab.id, updateRequest],
  );

  const handleSaveDocs = useCallback(
    async (docs: string | null) => {
      if (!tab.source) return;
      try {
        await updateRequestDocs(tab.source.collection, tab.source.path, docs);
        updateRequest(tab.id, { docs });
      } catch (err) {
        console.error('[RequestPanel] save docs failed:', err);
      }
    },
    [tab.id, tab.source, updateRequest],
  );

  const tabDefs = useMemo(
    () => [
      {
        value: 'params',
        label: (
          <>
            Params
            {enabledParamCount > 0 && (
              <span className='ml-1 inline-flex items-center justify-center min-w-4.5 h-4.5 rounded-full bg-muted px-1.5 text-xs font-semibold'>
                {enabledParamCount}
              </span>
            )}
          </>
        ),
        isActive: activeSection === 'params',
        onClick: () => setActiveSection('params'),
      },
      {
        value: 'headers',
        label: (
          <>
            Headers
            {enabledHeaderCount > 0 && (
              <span className='ml-1 inline-flex items-center justify-center min-w-4.5 h-4.5 rounded-full bg-muted px-1.5 text-xs font-semibold'>
                {enabledHeaderCount}
              </span>
            )}
          </>
        ),
        isActive: activeSection === 'headers',
        onClick: () => setActiveSection('headers'),
      },
      {
        value: 'body',
        label: (
          <>
            Body
            {request.body.mode !== 'none' && (
              <span className='ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary' />
            )}
          </>
        ),
        isActive: activeSection === 'body',
        onClick: () => setActiveSection('body'),
      },
      {
        value: 'auth',
        label: (
          <>
            Auth
            {request.auth.authType !== 'none' && (
              <span className='ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary' />
            )}
          </>
        ),
        isActive: activeSection === 'auth',
        onClick: () => setActiveSection('auth'),
      },
      {
        value: 'variables',
        label: (
          <>
            Variables
            {requestVarCount > 0 && (
              <span className='ml-1 inline-flex items-center justify-center min-w-4.5 h-4.5 rounded-full bg-muted px-1.5 text-xs font-semibold'>
                {requestVarCount}
              </span>
            )}
          </>
        ),
        isActive: activeSection === 'variables',
        onClick: () => setActiveSection('variables'),
      },
      {
        value: 'docs',
        label: (
          <>
            Docs
            {request.docs && (
              <span className='ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary' />
            )}
          </>
        ),
        isActive: activeSection === 'docs',
        onClick: () => setActiveSection('docs'),
      },
    ],
    [
      activeSection,
      enabledParamCount,
      enabledHeaderCount,
      request.body.mode,
      request.auth.authType,
      requestVarCount,
      request.docs,
    ],
  );

  const tabRightContent = useMemo(() => {
    if (activeSection === 'body') {
      return (
        <Select
          value={request.body.mode}
          onValueChange={(val) =>
            updateRequest(tab.id, {
              body: { ...request.body, mode: val as BodyState['mode'] },
            })
          }
        >
          <SelectTrigger className='h-7 w-30 text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {BODY_MODES.map((m) => (
              <SelectItem key={m.value} value={m.value} className='text-xs'>
                {m.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    }
    if (activeSection === 'auth') {
      return (
        <Select
          value={request.auth.authType}
          onValueChange={(val) => handleAuthTypeChange(val as AuthState['authType'])}
        >
          <SelectTrigger className='h-7 w-40 text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {authTypeOptions.map((t) => (
              <SelectItem key={t.value} value={t.value} className='text-xs'>
                {t.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    }
    if (activeSection === 'docs') {
      return (
        <Tabs value={docMode} onValueChange={(v) => setDocMode(v as 'edit' | 'preview')}>
          <TabsList className='h-6'>
            <TabsTrigger value='edit' className='text-[10px] px-2.5 py-0.5'>
              Edit
            </TabsTrigger>
            <TabsTrigger value='preview' className='text-[10px] px-2.5 py-0.5'>
              Preview
            </TabsTrigger>
          </TabsList>
        </Tabs>
      );
    }
    return undefined;
  }, [
    activeSection,
    request.body,
    request.auth.authType,
    tab.id,
    updateRequest,
    handleAuthTypeChange,
    authTypeOptions,
    docMode,
  ]);

  return (
    <div ref={containerRef} className='flex h-full flex-col overflow-hidden bg-transparent'>
      {/* ── Request area ── */}
      <div
        className='flex flex-col overflow-hidden bg-card/80 h-(--req-h) min-h-[20%] max-h-[80%]'
        style={{ '--req-h': `${requestHeight}%` } as React.CSSProperties}
      >
        {/* URL bar. */}
        <div className='flex items-center gap-2 border-b border-border/70 px-3 py-2 bg-card/70 backdrop-blur-sm'>
          <Select
            value={request.method}
            onValueChange={(val) => updateRequest(tab.id, { method: val as HttpMethod })}
          >
            <SelectTrigger
              className={cn('h-8 w-28 text-sm font-semibold', METHOD_TEXT_COLOR[request.method])}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {METHODS.map((m) => (
                <SelectItem
                  key={m}
                  value={m}
                  className={cn('text-sm font-semibold', METHOD_TEXT_COLOR[m])}
                >
                  {m}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => {
              setUrlError('');
              handleUrlChange(val);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') send(request);
            }}
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
            placeholder='https://api.example.com/resource'
            scopedContext={scopedContext}
          />

          <Button
            size='sm'
            className='h-8 px-3'
            disabled={sending}
            onClick={() => {
              const url = request.url.trim();
              if (!url) {
                setUrlError('URL is required');
                return;
              }
              try {
                new URL(url);
              } catch {
                setUrlError('Invalid URL — include http:// or https://');
                return;
              }
              setUrlError('');
              send(request);
            }}
          >
            <Send className='mr-1 h-3.5 w-3.5' />
            {sending ? 'Sending...' : 'Send'}
          </Button>

          <Button
            variant='outline'
            size='sm'
            className='h-7'
            onClick={() => setShowLoadTest(true)}
            title='Load test'
          >
            <Zap className='h-3.5 w-3.5' />
          </Button>

          {!tab.source && (
            <>
              <Button
                size='sm'
                variant='outline'
                className='h-7'
                onClick={() => setSaveToCollectionOpen(true)}
              >
                Save to Collection
              </Button>
              <SaveToCollectionDialog
                open={saveToCollectionOpen}
                tab={tab}
                onClose={() => setSaveToCollectionOpen(false)}
              />
            </>
          )}

          <SaveRequestButton tab={tab} groupId={_groupId} />
        </div>
        {urlError && <p className='text-xs text-destructive px-3 py-1'>{urlError}</p>}
        {curlImported && (
          <p className='text-xs text-green-600 dark:text-green-400 px-3 py-1'>Imported from cURL</p>
        )}

        {/* Section tabs. */}
        <div className='flex-1 flex flex-col min-h-0'>
          <BrunoTabBar tabs={tabDefs} rightContent={tabRightContent} />
          <div className='flex-1 overflow-auto p-3'>
            {activeSection === 'params' && (
              <div className='space-y-2'>
                <PathParamsPanel params={request.pathParams} onChange={handlePathParamsChange} />
                <QueryParamsEditor params={request.queryParams} onChange={handleParamsChange} />
              </div>
            )}
            {activeSection === 'headers' && (
              <HeadersEditor headers={request.headers} onChange={handleHeadersChange} />
            )}
            {activeSection === 'body' && (
              <BodyEditor body={request.body} onChange={handleBodyChange} />
            )}
            {activeSection === 'auth' && (
              <AuthEditor auth={request.auth} onChange={handleAuthChange} />
            )}
            {activeSection === 'variables' &&
              (tab.source?.collection && tab.source?.path ? (
                <RequestVariablesPanel
                  collection={tab.source.collection}
                  requestPath={tab.source.path}
                  onVarCountChange={setRequestVarCount}
                />
              ) : (
                <p className='p-4 text-sm text-muted-foreground'>
                  Save this request to a collection before adding request variables.
                </p>
              ))}
            {activeSection === 'docs' && (
              <RequestDocsPanel
                docs={request.docs}
                mode={docMode}
                hasSource={!!(tab.source?.collection && tab.source?.path)}
                onSave={(docs) => {
                  void handleSaveDocs(docs);
                }}
                onSwitchToEdit={() => setDocMode('edit')}
              />
            )}
          </div>
        </div>
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
      <div className='flex-1 flex flex-col overflow-hidden bg-card/65 min-h-0'>
        {sending ? (
          <div className='flex flex-1 flex-col items-center justify-center gap-3'>
            <Loader2 className='h-5 w-5 animate-spin text-primary' />
            <p className='text-sm text-muted-foreground'>Sending request...</p>
          </div>
        ) : response ? (
          <ResponseBodyViewer response={response} />
        ) : (
          <div className='flex flex-1 flex-col items-center justify-center gap-3'>
            <RocketLiftOff className='w-24 h-24' />
            <p className='text-sm font-medium text-foreground'>Ready for liftoff</p>
            <p className='text-xs text-muted-foreground'>Send a request to see the response here</p>
            <p className='text-xs text-muted-foreground mt-1'>
              Press{' '}
              <kbd className='rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-2xs'>
                Ctrl+Enter
              </kbd>{' '}
              to send
            </p>
          </div>
        )}
      </div>

      <LoadTestDialog open={showLoadTest} onOpenChange={setShowLoadTest} request={request} />

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
