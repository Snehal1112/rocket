import { useCallback, useEffect, useMemo, useState } from 'react';
import { AuthEditor } from '@/components/request/AuthEditor';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { AUTH_NODE_TYPE_OPTIONS, authStateForType } from '@/lib/auth-type-defaults';
import { withCurrentAuthType } from '@/lib/auth-type-options';
import {
  fingerprintFor,
  flowAuthKey,
  flowAuthState,
  resetTokenOnConfigChange,
} from '@/lib/flow-auth';
import { toPersistedAuth } from '@/lib/persisted-auth';
import {
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
} from '@/lib/queries/environment-queries';
import {
  type CollectionVariable,
  type Environment,
  type FlowNodeKind,
  getCollectionSettings,
} from '@/lib/tauri-api';
import { buildScopedContext } from '@/lib/url-variables';
import { resolveWithContext } from '@/lib/variable-context';
import { useEnvStore } from '@/stores/env-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';
import { LabelField } from './LabelField';

type AuthKind = Extract<FlowNodeKind, { kind: 'Auth' }>;

// Stable defaults while a query has no data, so the variable context (and the
// token check that depends on it) is not rebuilt on every render.
const NO_ENVIRONMENTS: Environment[] = [];
const NO_PROCESS_ENV: Record<string, string> = {};

export function AuthNodeEditor({
  kind,
  onChange,
  collection,
  flowName,
  nodeId,
}: {
  kind: AuthKind;
  onChange: (kind: FlowNodeKind) => void;
  collection: string;
  flowName: string;
  nodeId: string;
}) {
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  // The same query-cache entry getActiveGlobalEnvName() reads for the pre-run
  // step, so the editor and the preflight build the same key.
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const key = flowAuthKey(collection, flowName, nodeId, activeEnvId, globalEnvName);
  const stored = useFlowAuthStore((s) => s.auths[key]);
  const setAuth = useFlowAuthStore((s) => s.setAuth);
  const environmentName = activeEnvId ?? undefined;
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const { data: environments = NO_ENVIRONMENTS } = useEnvironments(activeCollection);
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = NO_PROCESS_ENV } = useProcessEnvVars();
  const [collectionVars, setCollectionVars] = useState<CollectionVariable[]>([]);

  useEffect(() => {
    let cancelled = false;
    getCollectionSettings(collection)
      .then((s) => {
        if (!cancelled) setCollectionVars(s.variables);
      })
      .catch(() => {
        if (!cancelled) setCollectionVars([]);
      });
    return () => {
      cancelled = true;
    };
  }, [collection]);

  // The same scoped context the collection Authorization tab builds. The OAuth2
  // editor resolves {{vars}} from it before "Get New Access Token", because the
  // backend does not read collection-scoped environments.
  const variableContext = useMemo(() => {
    const envVars: Record<string, string> = {};
    const activeEnv = activeEnvId ? environments.find((e) => e.name === activeEnvId) : undefined;
    if (activeEnv) for (const v of activeEnv.variables) if (v.enabled) envVars[v.key] = v.value;
    const globalVars: Record<string, string> = globalEnv
      ? Object.fromEntries(
          globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
        )
      : {};
    return buildScopedContext({
      envVars,
      envLabel: activeEnvId ?? undefined,
      externalSecrets: activeEnv?.externalSecrets,
      globalVars,
      processEnvVars,
      collectionVars,
    });
  }, [activeEnvId, environments, globalEnv, processEnvVars, collectionVars]);

  // Resolves {{vars}} the way the OAuth2 editor does before "Get New Access
  // Token", so a token's fingerprint describes the values it was fetched with.
  const rv = useMemo(() => {
    const ctx = Object.fromEntries([...variableContext.entries()].map(([k, e]) => [k, e.value]));
    return (s: string) => resolveWithContext(s, ctx);
  }, [variableContext]);

  // The store holds the full state, including a fetched token. It is used only
  // while it matches the persisted configuration; otherwise (never edited, or
  // changed by undo or a reload) the editor shows the persisted auth. A token
  // fetched for other variable values is shown as no token.
  const state: AuthState = useMemo(
    () => flowAuthState(stored, kind.auth, rv),
    [stored, kind.auth, rv],
  );

  const handleAuthChange = useCallback(
    (next: AuthState) => {
      // A token fetched for the old configuration must not outlive an edit to it.
      const safe = resetTokenOnConfigChange(state, next);
      setAuth(key, safe, fingerprintFor(safe, rv));
      // Only the configuration is persisted: toPersistedAuth has no token field.
      onChange({ ...kind, auth: toPersistedAuth(safe) });
    },
    [key, kind, onChange, rv, setAuth, state],
  );

  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />

      <div className='flex items-start justify-between gap-3'>
        <div className='space-y-0.5'>
          <Label htmlFor='auth-node-apply' className='text-xs'>
            Apply to inherited auth
          </Label>
          <p className='text-xs text-muted-foreground'>
            Every request in this flow with no auth of its own (inherit or none) uses this
            credential. A request with its own auth keeps it.
          </p>
        </div>
        <Switch
          id='auth-node-apply'
          aria-label='Apply to inherited auth'
          checked={kind.applyToInherit}
          onCheckedChange={(applyToInherit) => onChange({ ...kind, applyToInherit })}
        />
      </div>

      <div className='space-y-1.5'>
        <Label htmlFor='auth-node-type' className='text-xs'>
          Auth type
        </Label>
        <Select
          value={state.authType}
          onValueChange={(t) =>
            handleAuthChange(authStateForType(t as AuthState['authType'], state))
          }
        >
          <SelectTrigger id='auth-node-type' aria-label='Auth type' className='h-8 text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {withCurrentAuthType(AUTH_NODE_TYPE_OPTIONS, state.authType).map((t) => (
              <SelectItem key={t.value} value={t.value} className='text-sm'>
                {t.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <AuthEditor
        auth={state}
        onChange={handleAuthChange}
        variableContext={variableContext}
        collection={collection}
        environmentName={environmentName}
      />
    </div>
  );
}
