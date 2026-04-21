import { ChevronDown, ChevronRight } from 'lucide-react';
import { useCallback, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { oauth2DecodeJwt, oauth2GetToken, oauth2RefreshToken } from '@/lib/tauri-api';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { AuthState } from '@/types/pane-types';
import { OAuth2AdditionalParams } from './OAuth2AdditionalParams';
import { OAuth2AdvancedSection } from './OAuth2AdvancedSection';
import { OAuth2ConfigSection } from './OAuth2ConfigSection';
import { OAuth2SettingsSection } from './OAuth2SettingsSection';
import { OAuth2TokenDisplay } from './OAuth2TokenDisplay';
import { OAuth2TokenSection } from './OAuth2TokenSection';

type OAuth2State = NonNullable<AuthState['oauth2']>;
type OAuth2GrantType = OAuth2State['grantType'];

interface OAuth2AuthEditorProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
  collection?: string;
  environmentName?: string;
  requestPath?: string;
}

export function OAuth2AuthEditor({
  oauth2: o,
  patchOAuth2,
  variableContext,
  onNavigateToSource,
  collection,
  environmentName,
  requestPath,
}: OAuth2AuthEditorProps) {
  const [gettingToken, setGettingToken] = useState(false);
  const [tokenError, setTokenError] = useState('');
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [additionalOpen, setAdditionalOpen] = useState(false);

  const handleGetToken = useCallback(async () => {
    setGettingToken(true);
    setTokenError('');
    try {
      const result = await oauth2GetToken({
        grantType: o.grantType,
        authorizationUrl: o.authorizationUrl || undefined,
        tokenUrl: o.tokenUrl || undefined,
        callbackUrl: o.callbackUrl || undefined,
        clientId: o.clientId,
        clientSecret: o.clientSecret || undefined,
        scope: o.scope || undefined,
        state: o.state || undefined,
        username: o.username || undefined,
        password: o.password || undefined,
        clientAuthentication: o.clientAuthentication,
        usePkce: o.usePkce,
        useSystemBrowser: o.useSystemBrowser,
        verifySsl: o.verifySsl,
        authParams: o.authParams.length ? o.authParams : undefined,
        tokenParams: o.tokenParams.length ? o.tokenParams : undefined,
        refreshParams: o.refreshParams.length ? o.refreshParams : undefined,
        collection,
        environmentName,
        requestPath,
      });
      patchOAuth2({
        accessToken: result.access_token,
        refreshToken: result.refresh_token || '',
        expiresIn: typeof result.expires_in === 'number' ? result.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
        idToken: result.id_token || '',
        tokenType: result.token_type || '',
        responseScope: result.scope || '',
        idTokenClaims: null,
      });
      if (result.id_token) {
        try {
          const claims = await oauth2DecodeJwt(result.id_token);
          patchOAuth2({ idTokenClaims: claims });
        } catch {
          // JWT decode is best-effort — an opaque ID token shouldn't break the flow.
        }
      }
    } catch (err) {
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally {
      setGettingToken(false);
    }
  }, [o, patchOAuth2, collection, environmentName, requestPath]);

  const handleRefreshToken = useCallback(async () => {
    if (!o.refreshToken || !o.tokenUrl) return;
    setGettingToken(true);
    setTokenError('');
    try {
      const result = await oauth2RefreshToken({
        refreshToken: o.refreshToken,
        tokenUrl: o.tokenUrl,
        refreshTokenUrl: o.refreshTokenUrl || undefined,
        clientId: o.clientId,
        clientSecret: o.clientSecret || undefined,
        scope: o.scope || undefined,
        clientAuthentication: o.clientAuthentication,
        verifySsl: o.verifySsl,
        refreshParams: o.refreshParams.length ? o.refreshParams : undefined,
        collection,
        environmentName,
        requestPath,
      });
      patchOAuth2({
        accessToken: result.access_token,
        refreshToken: result.refresh_token || o.refreshToken,
        expiresIn: typeof result.expires_in === 'number' ? result.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
        idToken: result.id_token || o.idToken,
        tokenType: result.token_type || o.tokenType,
        responseScope: result.scope || o.responseScope,
      });
      if (result.id_token) {
        try {
          const claims = await oauth2DecodeJwt(result.id_token);
          patchOAuth2({ idTokenClaims: claims });
        } catch {
          // Non-critical.
        }
      }
    } catch (err) {
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally {
      setGettingToken(false);
    }
  }, [o, patchOAuth2, collection, environmentName, requestPath]);

  const handleClearCache = useCallback(() => {
    patchOAuth2({
      accessToken: '',
      refreshToken: '',
      expiresIn: null,
      tokenAcquiredAt: null,
      idToken: '',
      tokenType: '',
      responseScope: '',
      idTokenClaims: null,
    });
    setTokenError('');
  }, [patchOAuth2]);

  const getTokenDisabled =
    gettingToken ||
    (o.grantType !== 'implicit' && !o.tokenUrl) ||
    ((o.grantType === 'authorization_code' || o.grantType === 'implicit') && !o.authorizationUrl);

  return (
    <div className='space-y-4'>
      <div>
        <Label className='mb-1 block'>Grant Type</Label>
        <Select
          value={o.grantType}
          onValueChange={(val) => patchOAuth2({ grantType: val as OAuth2GrantType })}
        >
          <SelectTrigger className='w-50 text-sm'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='client_credentials' className='text-sm'>
              Client Credentials
            </SelectItem>
            <SelectItem value='password' className='text-sm'>
              Password
            </SelectItem>
            <SelectItem value='authorization_code' className='text-sm'>
              Authorization Code
            </SelectItem>
            <SelectItem value='implicit' className='text-sm'>
              Implicit
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <OAuth2TokenDisplay oauth2={o} />

      <OAuth2ConfigSection
        oauth2={o}
        patchOAuth2={patchOAuth2}
        variableContext={variableContext}
        onNavigateToSource={onNavigateToSource}
      />

      <div className='border-t border-border/50 pt-3'>
        <OAuth2TokenSection
          oauth2={o}
          patchOAuth2={patchOAuth2}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      </div>

      {o.grantType !== 'implicit' && (
        <div>
          <button
            type='button'
            className='flex items-center gap-1 cursor-pointer text-sm text-muted-foreground hover:text-foreground py-1'
            onClick={() => setAdvancedOpen(!advancedOpen)}
          >
            {advancedOpen ? (
              <ChevronDown className='h-3.5 w-3.5' />
            ) : (
              <ChevronRight className='h-3.5 w-3.5' />
            )}
            Advanced Settings
          </button>
          {advancedOpen && (
            <div className='mt-2 pl-1'>
              <OAuth2AdvancedSection
                oauth2={o}
                patchOAuth2={patchOAuth2}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          )}
        </div>
      )}

      <div className='border-t border-border/50 pt-3'>
        <OAuth2SettingsSection oauth2={o} patchOAuth2={patchOAuth2} />
      </div>

      <div>
        <button
          type='button'
          className='flex items-center gap-1 cursor-pointer text-sm text-muted-foreground hover:text-foreground py-1'
          onClick={() => setAdditionalOpen(!additionalOpen)}
        >
          {additionalOpen ? (
            <ChevronDown className='h-3.5 w-3.5' />
          ) : (
            <ChevronRight className='h-3.5 w-3.5' />
          )}
          Additional Parameters
        </button>
        {additionalOpen && (
          <div className='mt-2'>
            <OAuth2AdditionalParams
              oauth2={o}
              patchOAuth2={patchOAuth2}
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
        )}
      </div>

      <div className='border-t border-border/50 pt-3 space-y-2'>
        <div className='flex gap-2'>
          <Button
            variant='outline'
            size='sm'
            className='text-sm'
            disabled={getTokenDisabled}
            onClick={handleGetToken}
          >
            {gettingToken ? 'Waiting...' : 'Get Access Token'}
          </Button>
          {o.refreshToken && (
            <Button
              variant='outline'
              size='sm'
              className='text-sm'
              disabled={gettingToken || !o.tokenUrl}
              onClick={handleRefreshToken}
            >
              Refresh
            </Button>
          )}
          {(o.accessToken || o.refreshToken || o.idToken) && (
            <Button
              variant='ghost'
              size='sm'
              className='text-sm'
              disabled={gettingToken}
              onClick={handleClearCache}
            >
              Clear Cache
            </Button>
          )}
        </div>
        {tokenError && <p className='text-xs text-destructive'>{tokenError}</p>}
      </div>
    </div>
  );
}
