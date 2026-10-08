import { useCallback, useMemo } from 'react';
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
import { useCollectionVariableContext } from '@/hooks/useCollectionVariableContext';
import { AUTH_NODE_TYPE_OPTIONS, authStateForType } from '@/lib/auth-type-defaults';
import { withCurrentAuthType } from '@/lib/auth-type-options';
import {
  flowAuthEntryAfterEdit,
  flowAuthKey,
  flowAuthResolver,
  flowAuthState,
} from '@/lib/flow-auth';
import { plaintextSecretFields } from '@/lib/flow-secrets';
import { toPersistedAuth } from '@/lib/persisted-auth';
import type { FlowNodeKind } from '@/lib/tauri-api';
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
  otherNodeApplies = false,
}: {
  kind: AuthKind;
  onChange: (kind: FlowNodeKind) => void;
  collection: string;
  flowName: string;
  nodeId: string;
  /** Another Auth node in the flow already applies to inherited auth. */
  otherNodeApplies?: boolean;
}) {
  // The flow's own collection, which the pre-run step also uses
  // (buildOAuth2VarContext), so both resolve the same environment. The hook is
  // also the single source of the editor's highlighting and OAuth2 resolution.
  const {
    variableContext,
    envVars,
    globalVars,
    collectionVars,
    processEnvVars,
    activeEnvId,
    globalEnvName,
  } = useCollectionVariableContext(collection);
  const applyBlocked = otherNodeApplies && !kind.applyToInherit;
  const key = flowAuthKey(collection, flowName, nodeId, activeEnvId, globalEnvName);
  const stored = useFlowAuthStore((s) => s.auths[key]);
  const setAuth = useFlowAuthStore((s) => s.setAuth);
  const environmentName = activeEnvId ?? undefined;
  // Literal credentials in the persisted auth, by label. Never the values.
  const plaintextFields = plaintextSecretFields(kind.auth);

  // Resolves {{vars}} for the token fingerprint exactly as the pre-run step
  // does, so a token stored by either one is recognised by the other.
  const rv = useMemo(
    () => flowAuthResolver({ processEnvVars, globalVars, envVars, collectionVars }),
    [processEnvVars, globalVars, envVars, collectionVars],
  );

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
      const entry = flowAuthEntryAfterEdit(stored, state, next, rv);
      setAuth(key, entry.auth, entry.fingerprint);
      // Only the configuration is persisted: toPersistedAuth has no token field.
      onChange({ ...kind, auth: toPersistedAuth(entry.auth) });
    },
    [key, kind, onChange, rv, setAuth, state, stored],
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
          {applyBlocked && (
            <p id='auth-node-apply-note' className='text-xs text-amber-600 dark:text-amber-500'>
              Another Auth node already applies to inherited auth. Turn that one off first.
            </p>
          )}
        </div>
        <Switch
          id='auth-node-apply'
          aria-label='Apply to inherited auth'
          checked={kind.applyToInherit}
          disabled={applyBlocked}
          aria-describedby={applyBlocked ? 'auth-node-apply-note' : undefined}
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

      {plaintextFields.length > 0 && (
        <p role='note' className='text-xs text-amber-600 dark:text-amber-500'>
          This credential is saved as plain text in the flow file. Use a {'{{variable}}'} or a
          RocketVault reference instead. Plain text: {plaintextFields.join(', ')}.
        </p>
      )}

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
