import { Check, Loader2, Save } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AuthEditor } from '@/components/request/AuthEditor';
import { HeadersEditor } from '@/components/request/HeadersEditor';
import { Button } from '@/components/ui/button';
import { useSaveButton } from '@/hooks/use-save-button';
import { type Auth, saveCollectionSettings } from '@/lib/tauri-api';
import { buildScopedContext } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import type { AuthState, KeyValueEntry } from '@/types/pane-types';

interface CollectionSettingsDialogProps {
  collectionName: string;
  onClose: () => void;
}

type SettingsTab = 'auth' | 'headers';

const TABS: { label: string; value: SettingsTab }[] = [
  { label: 'Auth', value: 'auth' },
  { label: 'Headers', value: 'headers' },
];

// Default auth state with no authentication configured.
const DEFAULT_AUTH: AuthState = { authType: 'none' };

export function CollectionSettingsDialog({
  collectionName,
  onClose,
}: CollectionSettingsDialogProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>('auth');
  const [auth, setAuth] = useState<AuthState>(DEFAULT_AUTH);
  const [headers, setHeaders] = useState<KeyValueEntry[]>([]);

  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const environments = useEnvStore((s) => s.environments);
  const globalEnv = useEnvStore((s) => s.globalEnv);
  const processEnvVars = useEnvStore((s) => s.processEnvVars);
  const scopedContext = useMemo(() => {
    const envVars: Record<string, string> = {};
    if (activeEnvId) {
      const env = environments.find((e) => e.name === activeEnvId);
      if (env) for (const v of env.variables) if (v.enabled) envVars[v.key] = v.value;
    }
    const globalVars: Record<string, string> = globalEnv
      ? Object.fromEntries(globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]))
      : {};
    return buildScopedContext({ envVars, envLabel: activeEnvId ?? undefined, globalVars, processEnvVars });
  }, [activeEnvId, environments, globalEnv, processEnvVars]);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
    };
  }, []);

  const saveFn = useCallback(async () => {
    let apiAuth: Auth | undefined;
    if (auth.authType === 'basic')
      apiAuth = {
        authType: 'basic',
        username: auth.basic?.username ?? '',
        password: auth.basic?.password ?? '',
      };
    else if (auth.authType === 'bearer')
      apiAuth = { authType: 'bearer', token: auth.bearer?.token ?? '' };
    else if (auth.authType === 'api-key')
      apiAuth = {
        authType: 'api-key',
        key: auth.apiKey?.key ?? '',
        value: auth.apiKey?.value ?? '',
        placement: auth.apiKey?.addTo ?? 'header',
      };
    else apiAuth = undefined;

    await saveCollectionSettings(collectionName, {
      auth: apiAuth,
      headers: headers
        .filter((h) => h.key)
        .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
      variables: [],
    });
    // Let the success state show briefly before closing.
    closeTimerRef.current = setTimeout(() => onClose(), 1200);
  }, [auth, headers, collectionName, onClose]);

  const { state: saveState, trigger: triggerSave } = useSaveButton(
    saveFn,
    'Failed to save settings',
  );

  return (
    <div className='flex flex-col gap-4 p-4'>
      {/* Dialog header. */}
      <div>
        <h2 className='text-sm font-semibold'>{collectionName}</h2>
        <p className='text-xs text-muted-foreground'>Collection settings</p>
      </div>

      {/* Tab bar. */}
      <div className='flex gap-1 border-b border-border pb-0'>
        {TABS.map((tab) => (
          <button
            key={tab.value}
            type='button'
            onClick={() => setActiveTab(tab.value)}
            className={cn(
              'px-3 pb-2 text-xs font-medium transition-colors',
              activeTab === tab.value
                ? 'border-b-2 border-primary text-foreground'
                : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab content. */}
      <div className='min-h-[12rem]'>
        {activeTab === 'auth' && <AuthEditor auth={auth} onChange={setAuth} variableContext={scopedContext} collection={collectionName} environmentName={activeEnvId ?? undefined} />}
        {activeTab === 'headers' && <HeadersEditor headers={headers} onChange={setHeaders} />}
      </div>

      {/* Footer actions. */}
      <div className='flex justify-end gap-2'>
        <Button variant='ghost' size='sm' onClick={onClose}>
          Cancel
        </Button>
        <Button
          size='sm'
          onClick={() => void triggerSave()}
          disabled={saveState !== 'idle'}
          className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
        >
          {saveState === 'saving' ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : saveState === 'success' ? (
            <Check className='h-3.5 w-3.5' />
          ) : (
            <Save className='h-3.5 w-3.5' />
          )}
          {saveState === 'success' ? 'Saved' : 'Save'}
        </Button>
      </div>
    </div>
  );
}
