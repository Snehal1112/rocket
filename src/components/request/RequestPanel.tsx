import { Braces, Loader2, Send, ShieldCheck, Tag, X, Zap } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { SingleLineEditor } from '@/components/editor';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';
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
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useExecuteRequest } from '@/hooks/useExecuteRequest';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import type { ParsedCurl } from '@/lib/curl-parser';
import { findTabInTree } from '@/lib/pane-utils';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import {
  type CollectionVariable,
  getCollectionSettings,
  getFolderVariables,
  getRequestVariables,
  updateRequestDocs,
} from '@/lib/tauri-api';
import { buildUrl, extractPathParams, parseQueryParams, splitUrl } from '@/lib/url-params';
import type { VariableSource } from '@/lib/url-variables';
import { buildScopedContext } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import { useLayoutStore } from '@/stores/layout-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type {
  AuthState,
  BodyState,
  HttpMethod,
  KeyValueEntry,
  RequestSettings,
  RequestTab,
} from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';
import { AuthEditor } from './AuthEditor';
import { BodyEditor } from './BodyEditor';
import { HeadersEditor } from './HeadersEditor';
import { LoadTestTab } from './load-test/LoadTestTab';
import { PathParamsPanel } from './PathParamsPanel';
import { QueryParamsEditor } from './QueryParamsEditor';
import { RequestDocsPanel } from './RequestDocsPanel';
import { RequestVariablesPanel } from './RequestVariablesPanel';
import { RocketTabBar } from './RocketTabBar';
import { AssertionsTab } from './AssertionsTab';
import { SaveRequestButton } from './SaveRequestButton';
import { ScriptsTab } from './ScriptsTab';
import { SaveToCollectionDialog } from './SaveToCollectionDialog';

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

type SectionTab =
  | 'params'
  | 'headers'
  | 'body'
  | 'auth'
  | 'variables'
  | 'docs'
  | 'settings'
  | 'load-test'
  | 'scripts'
  | 'assertions';

interface RequestPanelProps {
  tab: RequestTab;
  groupId: string;
}

export function RequestPanel({ tab, groupId: _groupId }: RequestPanelProps) {
  const { request, response } = tab;
  const updateRequest = usePaneStore((s) => s.updateRequest);
  const requestLayout = useLayoutStore((s) => s.requestLayout);

  const { send, sending } = useExecuteRequest(tab.id);

  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  const [unsavedDialogOpen, setUnsavedDialogOpen] = useState(false);
  const [showLoadTest, setShowLoadTest] = useState(false);
  const [saveToCollectionOpen, setSaveToCollectionOpen] = useState(false);
  const [envDialogOpen, setEnvDialogOpen] = useState(false);
  const [urlError, setUrlError] = useState('');
  const [collectionVariables, setCollectionVariables] = useState<CollectionVariable[]>([]);
  const [folderVariables, setFolderVariables] = useState<CollectionVariable[]>([]);
  const [requestVariables, setRequestVariables] = useState<CollectionVariable[]>([]);
  const [curlImported, setCurlImported] = useState(false);
  const [requestVarCount, setRequestVarCount] = useState(0);
  const [tagInput, setTagInput] = useState('');

  // Resizable split: request height as percentage.
  const containerRef = useRef<HTMLDivElement>(null);
  const [requestHeight, setRequestHeight] = useState(55);
  const [isDragging, setIsDragging] = useState(false);
  const [requestWidth, setRequestWidth] = useState(50);

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

  // Fetch collection variables for the scoped variable context.
  useEffect(() => {
    if (!tab.source?.collection) {
      setCollectionVariables([]);
      return;
    }
    getCollectionSettings(tab.source.collection)
      .then((s) => {
        setCollectionVariables(s.variables);
      })
      .catch(() => {
        setCollectionVariables([]);
      });
  }, [tab.source?.collection]);

  // Fetch request-scoped and folder-scoped variables for the variable context.
  useEffect(() => {
    const collection = tab.source?.collection;
    const requestPath = tab.source?.path;
    if (!collection || !requestPath) {
      setRequestVariables([]);
      setFolderVariables([]);
      return;
    }
    // Derive folder path: parent directory of the request file.
    const folderPath = requestPath.includes('/')
      ? requestPath.slice(0, requestPath.lastIndexOf('/'))
      : '';
    getRequestVariables(collection, requestPath)
      .then(setRequestVariables)
      .catch(() => setRequestVariables([]));
    if (folderPath) {
      getFolderVariables(collection, folderPath)
        .then(setFolderVariables)
        .catch(() => setFolderVariables([]));
    } else {
      setFolderVariables([]);
    }
  }, [tab.source?.collection, tab.source?.path]);

  // Keyboard handler for the separator: ArrowUp/ArrowDown adjust split in 5% steps.
  const handleSeparatorKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setRequestHeight((h) => Math.min(90, Math.max(10, h - 5)));
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setRequestHeight((h) => Math.min(90, Math.max(10, h + 5)));
    }
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

  const handleVerticalSeparatorDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      setIsDragging(true);
      const container = containerRef.current;
      if (!container) return;

      const startX = e.clientX;
      const startWidth = requestWidth;
      const containerW = container.getBoundingClientRect().width;

      const onMove = (ev: PointerEvent) => {
        const delta = ev.clientX - startX;
        const pct = startWidth + (delta / containerW) * 100;
        setRequestWidth(Math.min(80, Math.max(20, pct)));
      };
      const onUp = () => {
        setIsDragging(false);
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    },
    [requestWidth],
  );

  const handleVerticalSeparatorKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      setRequestWidth((w) => Math.min(80, Math.max(20, w - 5)));
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      setRequestWidth((w) => Math.min(80, Math.max(20, w + 5)));
    }
  }, []);

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

      // Sync query and path params from the parsed URL.
      patch.queryParams = parseQueryParams(parsed.url);
      patch.pathParams = extractPathParams(parsed.url).map((name) => ({
        id: crypto.randomUUID(),
        key: name,
        value: '',
        enabled: true,
      }));

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
  // Docs and Settings don't use the response panel — expand to full height.
  const expandFull =
    activeSection === 'docs' ||
    activeSection === 'settings' ||
    activeSection === 'load-test' ||
    activeSection === 'scripts' ||
    activeSection === 'assertions';
  // Use safe access in case settings is absent on a request loaded from an older saved state.
  const settings = request.settings ?? {
    verifySsl: true,
    followRedirects: true,
    maxRedirects: 5,
    timeoutMs: 0,
    encodeUrl: true,
  };
  const settingsModified =
    !settings.verifySsl ||
    !settings.followRedirects ||
    settings.timeoutMs !== 0 ||
    settings.maxRedirects !== 5 ||
    !settings.encodeUrl ||
    (request.tags ?? []).length > 0;

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

  const activeEnvIdForScope = useEnvStore((s) => s.activeEnvId);
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = [] } = useEnvironments(activeCollection);
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars();

  // Build the scope-aware variable context for all editors in this panel.
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
      folderVars: folderVariables,
      requestVars: requestVariables,
    });
  }, [
    activeEnvIdForScope,
    environments,
    globalEnv,
    processEnvVars,
    collectionVariables,
    folderVariables,
    requestVariables,
  ]);

  const authTypeOptions = useMemo(
    () => (tab.source ? [INHERIT_AUTH_OPTION, ...BASE_AUTH_TYPES] : BASE_AUTH_TYPES),
    [tab.source],
  );

  const handleAuthTypeChange = useCallback(
    (authType: AuthState['authType']) => {
      const prev = request.auth;
      const next: AuthState = { authType };
      if (authType === 'basic') next.basic = prev.basic ?? { username: '', password: '' };
      if (authType === 'bearer') next.bearer = prev.bearer ?? { token: '' };
      if (authType === 'api-key')
        next.apiKey = prev.apiKey ?? { key: '', value: '', addTo: 'header' };
      if (authType === 'oauth2')
        next.oauth2 = prev.oauth2 ?? {
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
          usePkce: true,
          useSystemBrowser: false,
          tokenSource: 'accessToken',
          tokenId: '',
          refreshTokenUrl: '',
          autoFetchToken: true,
          autoRefreshToken: false,
          authParams: [],
          tokenParams: [],
          refreshParams: [],
          idToken: '',
          tokenType: '',
          responseScope: '',
          idTokenClaims: null,
          accessTokenClaims: null,
        };
      if (authType === 'aws-sig-v4')
        next.awsSigV4 = prev.awsSigV4 ?? {
          accessKey: '',
          secretKey: '',
          region: '',
          service: '',
          sessionToken: '',
        };
      updateRequest(tab.id, { auth: next });
    },
    [tab.id, updateRequest, request.auth],
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

  const handleSettingsChange = useCallback(
    (patch: Partial<RequestSettings>) => {
      const current = request.settings ?? {
        verifySsl: true,
        followRedirects: true,
        maxRedirects: 5,
        timeoutMs: 0,
        encodeUrl: true,
      };
      updateRequest(tab.id, { settings: { ...current, ...patch } });
    },
    [tab.id, updateRequest, request.settings],
  );

  const handleAddTag = useCallback(
    (raw: string) => {
      const tag = raw.trim().replace(/,/g, '');
      if (!tag) return;
      const current = request.tags ?? [];
      if (current.includes(tag)) return;
      updateRequest(tab.id, { tags: [...current, tag] });
      setTagInput('');
    },
    [tab.id, updateRequest, request.tags],
  );

  const handleRemoveTag = useCallback(
    (tag: string) => {
      const current = request.tags ?? [];
      updateRequest(tab.id, { tags: current.filter((t) => t !== tag) });
    },
    [tab.id, updateRequest, request.tags],
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

  const handleNavigateToSource = useCallback(
    (source: VariableSource | 'pathParam') => {
      switch (source) {
        case 'pathParam':
          setActiveSection('params');
          break;
        case 'request':
        case 'runtime':
          setActiveSection('variables');
          break;
        case 'environment':
          setEnvDialogOpen(true);
          break;
        case 'global': {
          const wsId = useWorkspaceStore.getState().activeWorkspaceId;
          if (wsId) usePaneStore.getState().openWorkspaceTabs(wsId, 'environments');
          break;
        }
        case 'collection': {
          const collection = tab.source?.collection;
          if (collection) {
            const found = usePaneStore.getState().openCollectionTab(collection, 'variables');
            if (!found) {
              toast.info('Open the collection tab to edit collection variables.');
            }
          }
          break;
        }
        default:
          // folder and process vars have no navigable destination in the current UI.
          break;
      }
    },
    [tab.source?.collection],
  );

  // Adapter for editors: they pass (source, key) but navigation only uses source.
  const handleEditorNavigateToSource = useCallback(
    (source: VariableSource | 'pathParam', _key: string) => {
      handleNavigateToSource(source);
    },
    [handleNavigateToSource],
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
        value: 'scripts',
        label: (
          <>
            Scripts
            {(request.preRequestScript || request.postResponseScript || request.testsScript) && (
              <span className='ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary' />
            )}
          </>
        ),
        isActive: activeSection === 'scripts',
        onClick: () => setActiveSection('scripts'),
      },
      {
        value: 'assertions',
        label: (
          <>
            Assertions
            {request.assertions.some((a) => !a.disabled) && (
              <span className='ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary' />
            )}
          </>
        ),
        isActive: activeSection === 'assertions',
        onClick: () => setActiveSection('assertions'),
      },
      {
        value: 'load-test',
        label: 'Load test',
        isActive: activeSection === 'load-test',
        onClick: () => setActiveSection('load-test'),
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
      {
        value: 'settings',
        label: (
          <>
            Settings
            {settingsModified && (
              <span className='ml-1.5 inline-block w-1.5 h-1.5 rounded-full bg-primary' />
            )}
          </>
        ),
        isActive: activeSection === 'settings',
        onClick: () => setActiveSection('settings'),
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
      settingsModified,
      request.assertions,
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

  const urlBar = (
    <>
      <div className='flex items-center gap-2 border-b border-border px-3 py-2 bg-card'>
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

        <SingleLineEditor
          value={request.url}
          onChange={(val) => {
            setUrlError('');
            handleUrlChange(val);
          }}
          onSubmit={() => send(request)}
          onCurlImport={handleCurlImport}
          variableContext={scopedContext}
          pathParams={pathParamMap}
          queryParams={queryParamMap}
          onPathParamChange={(key, val) => {
            const updated = request.pathParams.map((p) =>
              p.key === key ? { ...p, value: val } : p,
            );
            updateRequest(tab.id, { pathParams: updated });
          }}
          onNavigateToSource={handleEditorNavigateToSource}
          placeholder='Enter URL or paste a cURL request'
          className='flex-1'
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
            // Skip URL format check when the URL has unresolved template vars;
            // the backend resolves them before making the HTTP call.
            if (!url.includes('{{')) {
              try {
                new URL(url);
              } catch {
                setUrlError('Invalid URL — include http:// or https://');
                return;
              }
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
          aria-label='Load test'
        >
          <Zap className='h-3.5 w-3.5' aria-hidden='true' />
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
    </>
  );

  const sectionTabs = (
    <div className='flex-1 flex flex-col min-h-0 bg-card'>
      <RocketTabBar tabs={tabDefs} rightContent={tabRightContent} />
      {activeSection === 'docs' ? (
        <div className='flex-1 flex flex-col overflow-hidden p-3 min-h-0'>
          <RequestDocsPanel
            docs={request.docs}
            mode={docMode}
            hasSource={!!(tab.source?.collection && tab.source?.path)}
            onSave={(docs) => {
              void handleSaveDocs(docs);
            }}
            onSwitchToEdit={() => setDocMode('edit')}
          />
        </div>
      ) : null}
      {activeSection === 'load-test' ? (
        <div className='flex-1 min-h-0 overflow-hidden'>
          <LoadTestTab request={request} tabId={tab.id} />
        </div>
      ) : null}
      {activeSection === 'scripts' ? (
        <div className='flex-1 min-h-0 overflow-hidden'>
          <ScriptsTab
            preRequestScript={request.preRequestScript ?? ''}
            postResponseScript={request.postResponseScript ?? ''}
            testsScript={request.testsScript ?? ''}
            onChangePreRequest={(v) => updateRequest(tab.id, { preRequestScript: v })}
            onChangePostResponse={(v) => updateRequest(tab.id, { postResponseScript: v })}
            onChangeTests={(v) => updateRequest(tab.id, { testsScript: v })}
          />
        </div>
      ) : null}
      {activeSection === 'assertions' ? (
        <div className='flex-1 min-h-0 overflow-hidden'>
          <AssertionsTab
            assertions={request.assertions}
            onChange={(newAssertions) => updateRequest(tab.id, { assertions: newAssertions })}
          />
        </div>
      ) : null}
      <div
        className={
          activeSection === 'docs' ||
          activeSection === 'load-test' ||
          activeSection === 'scripts' ||
          activeSection === 'assertions'
            ? 'hidden'
            : 'flex-1 overflow-auto p-3'
        }
      >
        {activeSection === 'params' && (
          <div className='space-y-3'>
            <PathParamsPanel
              params={request.pathParams}
              onChange={handlePathParamsChange}
              variableContext={scopedContext}
              onNavigateToSource={handleEditorNavigateToSource}
            />
            <QueryParamsEditor
              params={request.queryParams}
              onChange={handleParamsChange}
              variableContext={scopedContext}
              onNavigateToSource={handleEditorNavigateToSource}
            />
          </div>
        )}
        {activeSection === 'headers' && (
          <HeadersEditor
            headers={request.headers}
            onChange={handleHeadersChange}
            variableContext={scopedContext}
            onNavigateToSource={handleEditorNavigateToSource}
          />
        )}
        {activeSection === 'body' && (
          <BodyEditor
            body={request.body}
            onChange={handleBodyChange}
            variableContext={scopedContext}
            onNavigateToSource={handleEditorNavigateToSource}
          />
        )}
        {activeSection === 'auth' && (
          <AuthEditor
            auth={request.auth}
            onChange={handleAuthChange}
            variableContext={scopedContext}
            onNavigateToSource={handleEditorNavigateToSource}
            collection={tab.source?.collection}
            environmentName={activeEnvIdForScope ?? undefined}
            requestPath={tab.source?.path}
          />
        )}
        {activeSection === 'variables' &&
          (tab.source?.collection && tab.source?.path ? (
            <RequestVariablesPanel
              collection={tab.source.collection}
              requestPath={tab.source.path}
              onVarCountChange={setRequestVarCount}
            />
          ) : (
            <div className='flex flex-col items-center justify-center gap-4 py-10 text-center'>
              <div className='h-12 w-12 rounded-xl bg-muted/40 border border-border/50 flex items-center justify-center'>
                <Braces className='h-5 w-5 text-muted-foreground/30' />
              </div>
              <div className='space-y-1.5'>
                <p className='text-xs font-medium text-muted-foreground/70'>
                  No collection attached
                </p>
                <p className='text-[11px] text-muted-foreground/40 max-w-50 leading-relaxed'>
                  Save this request to a collection to define request-scoped variables.
                </p>
              </div>
            </div>
          ))}
        {activeSection === 'settings' && (
          <ScrollArea className='h-full'>
            <div className='p-6 max-w-2xl mx-auto space-y-4'>
              {/* Tags */}
              <Card>
                <CardContent className='p-4 space-y-3'>
                  <div className='flex items-center gap-2 mb-1'>
                    <Tag className='h-3.5 w-3.5 text-muted-foreground' />
                    <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
                      Tags
                    </span>
                  </div>
                  <Input
                    placeholder='e.g. smoke, regression etc'
                    value={tagInput}
                    onChange={(e) => setTagInput(e.target.value.replace(/,/g, ''))}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ',') {
                        e.preventDefault();
                        handleAddTag(tagInput);
                      }
                    }}
                    className='h-8 text-sm'
                  />
                  {(request.tags ?? []).length > 0 && (
                    <div className='flex flex-wrap gap-1.5 pt-0.5'>
                      {(request.tags ?? []).map((tag) => (
                        <Badge
                          key={tag}
                          variant='secondary'
                          className='gap-1 pl-2 pr-1 py-0.5 text-xs font-normal'
                        >
                          <Tag className='h-2.5 w-2.5 text-muted-foreground' />
                          {tag}
                          <Button
                            type='button'
                            variant='ghost'
                            size='icon'
                            onClick={() => handleRemoveTag(tag)}
                            className='ml-0.5 h-4 w-4 rounded-sm opacity-60 hover:opacity-100'
                            aria-label={`Remove tag ${tag}`}
                          >
                            <X className='h-2.5 w-2.5' />
                          </Button>
                        </Badge>
                      ))}
                    </div>
                  )}
                </CardContent>
              </Card>

              {/* Request settings */}
              <Card>
                <CardContent className='p-4 space-y-0 divide-y divide-border'>
                  {/* URL Encoding */}
                  <div className='flex items-center justify-between py-3'>
                    <div>
                      <p className='text-sm font-medium'>URL Encoding</p>
                      <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                        Automatically encode query parameters in the URL
                      </p>
                    </div>
                    <Switch
                      checked={settings.encodeUrl}
                      onCheckedChange={(checked) => handleSettingsChange({ encodeUrl: checked })}
                    />
                  </div>

                  {/* Follow Redirects */}
                  <div className='flex items-center justify-between py-3'>
                    <div>
                      <p className='text-sm font-medium'>Automatically Follow Redirects</p>
                      <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                        Follow HTTP redirects automatically
                      </p>
                    </div>
                    <Switch
                      checked={settings.followRedirects}
                      onCheckedChange={(checked) =>
                        handleSettingsChange({ followRedirects: checked })
                      }
                    />
                  </div>

                  {/* Max Redirects */}
                  <div className='flex items-center justify-between py-3'>
                    <div>
                      <p
                        className={cn(
                          'text-sm font-medium',
                          !settings.followRedirects && 'text-muted-foreground',
                        )}
                      >
                        Max Redirects
                      </p>
                      <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                        Set a limit for the number of redirects to follow
                      </p>
                    </div>
                    <Input
                      type='number'
                      min={0}
                      disabled={!settings.followRedirects}
                      className='h-8 w-24 text-sm text-right tabular-nums'
                      value={settings.maxRedirects}
                      onChange={(e) => {
                        const val = Number(e.target.value);
                        if (!Number.isNaN(val) && val >= 0) {
                          handleSettingsChange({ maxRedirects: val });
                        }
                      }}
                    />
                  </div>

                  {/* Timeout */}
                  <div className='flex items-center justify-between py-3'>
                    <div>
                      <p className='text-sm font-medium'>Timeout (ms)</p>
                      <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                        Set maximum time to wait before aborting the request
                      </p>
                    </div>
                    <div className='flex items-center gap-1.5'>
                      <Input
                        type='number'
                        min={0}
                        className='h-8 w-24 text-sm text-right tabular-nums'
                        value={settings.timeoutMs}
                        onChange={(e) => {
                          const val = Number(e.target.value);
                          if (!Number.isNaN(val) && val >= 0) {
                            handleSettingsChange({ timeoutMs: val });
                          }
                        }}
                      />
                      {settings.timeoutMs > 0 && (
                        <Button
                          type='button'
                          variant='ghost'
                          size='icon'
                          onClick={() => handleSettingsChange({ timeoutMs: 0 })}
                          className='h-6 w-6 text-muted-foreground hover:text-foreground'
                          aria-label='Clear timeout'
                        >
                          <X className='h-3.5 w-3.5' />
                        </Button>
                      )}
                    </div>
                  </div>
                </CardContent>
              </Card>

              {/* Security */}
              <Card>
                <CardContent className='p-4 space-y-3'>
                  <div className='flex items-center gap-2 mb-1'>
                    <ShieldCheck className='h-3.5 w-3.5 text-muted-foreground' />
                    <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
                      Security
                    </span>
                  </div>
                  <label
                    htmlFor='verify-ssl'
                    className='flex items-center gap-2.5 rounded-md px-2 py-1.5 -mx-1 cursor-pointer transition-colors hover:bg-muted/60'
                  >
                    <Checkbox
                      id='verify-ssl'
                      checked={settings.verifySsl}
                      onCheckedChange={(checked) => handleSettingsChange({ verifySsl: !!checked })}
                    />
                    <div>
                      <span className='text-sm'>Verify SSL certificate</span>
                      <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                        Validate the server's TLS certificate chain.
                      </p>
                    </div>
                  </label>
                </CardContent>
              </Card>
            </div>
          </ScrollArea>
        )}
      </div>
    </div>
  );

  const responseArea = sending ? (
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
  );

  const dialogs = (
    <>
      <LoadTestDialog
        open={showLoadTest}
        onOpenChange={setShowLoadTest}
        request={request}
        tabId={tab.id}
      />
      <EnvironmentDialog open={envDialogOpen} onOpenChange={setEnvDialogOpen} />
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
    </>
  );

  if (requestLayout === 'side-by-side') {
    return (
      <div ref={containerRef} className='flex h-full flex-col overflow-hidden bg-transparent'>
        {urlBar}
        <div className='flex flex-1 min-h-0'>
          {/* Request side — full width when response is suppressed. */}
          <div
            className={cn(
              'flex flex-col overflow-hidden bg-card',
              expandFull ? 'flex-1' : 'min-w-[20%] max-w-[80%]',
            )}
            style={expandFull ? undefined : { width: `${requestWidth}%` }}
          >
            {sectionTabs}
          </div>

          {!expandFull && (
            <>
              {/* Vertical separator */}
              {/* biome-ignore lint/a11y/useSemanticElements: drag splitter cannot be an <hr> */}
              <div
                role='separator'
                tabIndex={0}
                aria-orientation='vertical'
                aria-label='Resize request and response panels'
                aria-valuemin={20}
                aria-valuemax={80}
                aria-valuenow={Math.round(requestWidth)}
                onPointerDown={handleVerticalSeparatorDown}
                onKeyDown={handleVerticalSeparatorKeyDown}
                className={cn(
                  'w-3 flex items-center justify-center cursor-col-resize select-none border-x transition-colors',
                  'focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-1',
                  isDragging
                    ? 'bg-primary/15 border-primary/50'
                    : 'bg-muted/30 border-border hover:bg-accent/50 hover:border-primary/40',
                )}
              >
                <div
                  className={cn(
                    'rounded-full transition-all',
                    isDragging ? 'h-24 w-1.5 bg-primary' : 'h-16 w-1 bg-muted-foreground/40',
                  )}
                />
              </div>

              {/* Response side */}
              <div className='flex-1 flex flex-col overflow-hidden bg-card min-w-0'>
                {responseArea}
              </div>
            </>
          )}
        </div>
        {dialogs}
      </div>
    );
  }

  return (
    <div ref={containerRef} className='flex h-full flex-col overflow-hidden bg-transparent'>
      {/* ── Request area ── */}
      <div
        className={cn(
          'flex flex-col overflow-hidden bg-card',
          expandFull ? 'flex-1' : 'h-(--req-h) min-h-[20%] max-h-[80%]',
        )}
        style={expandFull ? undefined : ({ '--req-h': `${requestHeight}%` } as React.CSSProperties)}
      >
        {urlBar}
        {sectionTabs}
      </div>

      {/* ── Drag separator and response area — hidden on Docs/Settings tabs. ── */}
      {!expandFull && (
        <>
          {/* biome-ignore lint/a11y/useSemanticElements: drag splitter cannot be an <hr> */}
          <div
            role='separator'
            tabIndex={0}
            aria-orientation='horizontal'
            aria-label='Resize request and response panels'
            aria-valuemin={10}
            aria-valuemax={90}
            aria-valuenow={Math.round(requestHeight)}
            onPointerDown={handleSeparatorDown}
            onKeyDown={handleSeparatorKeyDown}
            className={cn(
              'h-3 flex items-center justify-center cursor-row-resize select-none border-y transition-colors',
              'focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-1',
              isDragging
                ? 'bg-primary/15 border-primary/50'
                : 'bg-muted/30 border-border hover:bg-accent/50 hover:border-primary/40',
            )}
          >
            <div
              className={cn(
                'rounded-full transition-all',
                isDragging ? 'w-24 h-1.5 bg-primary' : 'w-16 h-1 bg-muted-foreground/40',
              )}
            />
          </div>

          <div className='flex-1 flex flex-col overflow-hidden bg-card min-h-0'>{responseArea}</div>
        </>
      )}
      {dialogs}
    </div>
  );
}
