import { ShieldCheck } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AuthEditor } from '@/components/request/AuthEditor';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useFolderVariableContext } from '@/hooks/useFolderVariableContext';
import { authStateForType } from '@/lib/auth-type-defaults';
import {
  folderAuthToState,
  folderAuthTypeOptions,
  folderChainPath,
  restoreOAuth2Tokens,
  stateToFolderAuth,
} from '@/lib/folder-settings-convert';
import type { FolderSettings } from '@/lib/tauri-api';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import type { AuthState } from '@/types/pane-types';
import type { FolderSectionProps } from './sections';

const INHERIT_NOTE =
  'No authorization is set on this folder. Requests below it that are set to Inherit use the nearest parent folder with authorization, then the collection. A folder cannot switch authorization off for the requests below it.';

const NONE_ON_DISK_NOTE =
  'This folder has "No Auth" set in its folder.yml. It has no effect: requests below it still inherit from the parent folder or the collection. Choose Inherit to remove it.';

function loadAuthState(
  collectionName: string,
  folderPath: string,
  persisted: FolderSettings['auth'],
): AuthState {
  return restoreOAuth2Tokens(
    folderAuthToState(persisted),
    useFolderAuthStore.getState().getFolderAuth(collectionName, folderPath),
  );
}

/** Serialised persisted auth, with no folder auth as `null`, for change detection. */
function authFingerprint(auth: FolderSettings['auth'] | null): string {
  return JSON.stringify(auth ?? null);
}

export function AuthSection({
  collectionName,
  folderPath,
  settings,
  onChange,
}: FolderSectionProps) {
  // The editor state lives here because OAuth2 tokens are not part of the persisted shape.
  const [auth, setAuth] = useState<AuthState>(() =>
    loadAuthState(collectionName, folderPath, settings.auth),
  );
  const { variableContext, environmentName } = useFolderVariableContext(
    collectionName,
    folderPath,
    settings.variables,
  );

  // Refs so the edit handler can write to the token store even when this component has
  // unmounted, for example when the user switches tabs during an OAuth2 browser flow.
  const targetRef = useRef({ collectionName, folderPath });
  targetRef.current = { collectionName, folderPath };
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // biome-ignore lint/correctness/useExhaustiveDependencies: only an outside change to settings.auth or a new folder resets the editor; the local state is compared inside.
  useEffect(() => {
    if (authFingerprint(settings.auth) !== authFingerprint(stateToFolderAuth(auth))) {
      setAuth(loadAuthState(collectionName, folderPath, settings.auth));
    }
  }, [settings.auth, collectionName, folderPath]);

  const commit = useCallback((next: AuthState) => {
    setAuth(next);
    const target = targetRef.current;
    useFolderAuthStore.getState().setFolderAuth(target.collectionName, target.folderPath, next);
    onChangeRef.current({ auth: stateToFolderAuth(next) });
  }, []);

  const handleTypeChange = useCallback(
    (authType: AuthState['authType']) => commit(authStateForType(authType, auth)),
    [auth, commit],
  );

  return (
    <div className='p-4 max-w-2xl'>
      <Card>
        <CardHeader className='pb-3 pt-4 px-4 border-b border-border/40'>
          <div className='flex items-center justify-between'>
            <div className='flex items-center gap-2'>
              <ShieldCheck className='h-4 w-4 text-muted-foreground' />
              <CardTitle className='text-sm font-medium'>Authorization</CardTitle>
            </div>
            <div className='flex items-center gap-2'>
              <span className='text-xs text-muted-foreground shrink-0'>Auth type</span>
              <Select
                value={auth.authType}
                onValueChange={(v) => handleTypeChange(v as AuthState['authType'])}
              >
                <SelectTrigger aria-label='Auth type' className='h-7 w-40 text-xs'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {folderAuthTypeOptions(auth.authType).map((t) => (
                    <SelectItem key={t.value} value={t.value} className='text-sm'>
                      {t.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
        </CardHeader>
        <CardContent className='p-4 space-y-4'>
          {auth.authType === 'none' && (
            <Card className='bg-muted/50'>
              <CardContent className='px-3 py-2.5'>
                <p className='text-xs text-muted-foreground'>{NONE_ON_DISK_NOTE}</p>
              </CardContent>
            </Card>
          )}
          <AuthEditor
            auth={auth}
            onChange={commit}
            variableContext={variableContext}
            collection={collectionName}
            environmentName={environmentName}
            requestPath={folderChainPath(folderPath) || undefined}
            inheritMessage={INHERIT_NOTE}
          />
        </CardContent>
      </Card>
    </div>
  );
}
