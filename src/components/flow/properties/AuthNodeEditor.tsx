import { useCallback, useMemo } from 'react';
import { AuthEditor } from '@/components/request/AuthEditor';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { flowAuthKey, resetTokenOnConfigChange } from '@/lib/flow-auth';
import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';
import { LabelField } from './LabelField';

type AuthKind = Extract<FlowNodeKind, { kind: 'Auth' }>;

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
  const key = flowAuthKey(collection, flowName, nodeId);
  const stored = useFlowAuthStore((s) => s.auths[key]);
  const setAuth = useFlowAuthStore((s) => s.setAuth);
  const environmentName = useEnvStore((s) => s.activeEnvId) ?? undefined;

  // The store holds the full state, including a fetched token. A node that was
  // never edited in this session falls back to its persisted configuration.
  const state: AuthState = useMemo(
    () => stored ?? fromPersistedAuth(kind.auth),
    [stored, kind.auth],
  );

  const handleAuthChange = useCallback(
    (next: AuthState) => {
      // A token fetched for the old configuration must not outlive an edit to it.
      const safe = resetTokenOnConfigChange(state, next);
      setAuth(key, safe);
      // Only the configuration is persisted: toPersistedAuth has no token field.
      onChange({ ...kind, auth: toPersistedAuth(safe) });
    },
    [key, kind, onChange, setAuth, state],
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
            Every request in this flow whose auth is set to inherit uses this credential. A request
            with its own auth keeps it.
          </p>
        </div>
        <Switch
          id='auth-node-apply'
          aria-label='Apply to inherited auth'
          checked={kind.applyToInherit}
          onCheckedChange={(applyToInherit) => onChange({ ...kind, applyToInherit })}
        />
      </div>

      <AuthEditor
        auth={state}
        onChange={handleAuthChange}
        collection={collection}
        environmentName={environmentName}
      />
    </div>
  );
}
