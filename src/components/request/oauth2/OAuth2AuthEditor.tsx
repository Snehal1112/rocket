import {
  ChevronDown,
  ChevronRight,
  KeyRound,
  ListPlus,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Workflow,
} from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
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
import { OAuth2SectionHeader } from './OAuth2SectionHeader';
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
  const [additionalOpen, setAdditionalOpen] = useState(
    () => o.authParams.length > 0 || o.tokenParams.length > 0 || o.refreshParams.length > 0,
  );
  const tokenDisplayRef = useRef<HTMLDivElement>(null);

  // Always holds the latest patchOAuth2 so async handlers never use a stale closure.
  const patchOAuth2Ref = useRef(patchOAuth2);
  patchOAuth2Ref.current = patchOAuth2;

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
      patchOAuth2Ref.current({
        accessToken: result.access_token,
        refreshToken: result.refresh_token || '',
        expiresIn: typeof result.expires_in === 'number' ? result.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
        idToken: result.id_token || '',
        tokenType: result.token_type || '',
        responseScope: result.scope || '',
        idTokenClaims: null,
      });
      // Scroll the token card into view after React re-renders it.
      setTimeout(
        () => tokenDisplayRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }),
        0,
      );
      if (result.id_token) {
        try {
          const claims = await oauth2DecodeJwt(result.id_token);
          patchOAuth2Ref.current({ idTokenClaims: claims });
        } catch {
          // JWT decode is best-effort — an opaque ID token shouldn't break the flow.
        }
      }
    } catch (err) {
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally {
      setGettingToken(false);
    }
  }, [o, collection, environmentName, requestPath]);

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
      patchOAuth2Ref.current({
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
          patchOAuth2Ref.current({ idTokenClaims: claims });
        } catch {
          // Non-critical.
        }
      }
    } catch (err) {
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally {
      setGettingToken(false);
    }
  }, [o, collection, environmentName, requestPath]);

  const handleClearCache = useCallback(() => {
    patchOAuth2Ref.current({
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
  }, []);

  const getTokenDisabled =
    gettingToken ||
    (o.grantType !== 'implicit' && !o.tokenUrl) ||
    ((o.grantType === 'authorization_code' || o.grantType === 'implicit') && !o.authorizationUrl);

  const grantLabel = GRANT_LABELS[o.grantType];
  const hasAnyToken = !!(o.accessToken || o.refreshToken || o.idToken);

  const advancedId = 'oauth2-advanced-content';
  const additionalId = 'oauth2-additional-content';

  return (
    <div className='space-y-0'>
      {/* ── Flow ── */}
      <div className='pb-4 mb-4 border-b border-border/40'>
        <div className='flex items-start gap-3'>
          <div className='flex items-center gap-2 pt-1 shrink-0'>
            <Workflow className='h-3.5 w-3.5 text-foreground/60' aria-hidden='true' />
            <label
              htmlFor='oauth2-grant-type'
              className='text-xs font-semibold uppercase tracking-[0.06em] text-foreground/70'
            >
              Flow
            </label>
          </div>
          <div className='flex-1 min-w-0'>
            <Select
              value={o.grantType}
              onValueChange={(val) => patchOAuth2({ grantType: val as OAuth2GrantType })}
            >
              <SelectTrigger id='oauth2-grant-type' className='h-8 w-full text-sm'>
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
            <p className='mt-1.5 text-xs text-muted-foreground leading-relaxed'>
              {grantLabel.hint}
            </p>
          </div>
        </div>
      </div>

      {/* ── Token display ── */}
      <div ref={tokenDisplayRef} className='mb-4'>
        <OAuth2TokenDisplay oauth2={o} />
      </div>

      {/* ── Configuration ── */}
      <section aria-labelledby='oauth2-config-heading' className='mb-4'>
        <div className='flex items-center gap-2 pb-2.5 mb-3 border-b border-border/30'>
          <OAuth2SectionHeader icon={ShieldCheck} title='Configuration' />
        </div>
        <OAuth2ConfigSection
          oauth2={o}
          patchOAuth2={patchOAuth2}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      </section>

      {/* ── Token Handling ── */}
      <section
        aria-labelledby='oauth2-token-heading'
        className='mb-4 pt-4 border-t border-border/40'
      >
        <div className='flex items-center gap-2 pb-2.5 mb-3 border-b border-border/30'>
          <OAuth2SectionHeader icon={KeyRound} title='Token Handling' />
        </div>
        <OAuth2TokenSection
          oauth2={o}
          patchOAuth2={patchOAuth2}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      </section>

      {/* ── Advanced (collapsible) ── */}
      {o.grantType !== 'implicit' && (
        <section className='pt-1 border-t border-border/40'>
          <button
            type='button'
            className='flex w-full items-center gap-2 py-2.5 px-0.5 text-left rounded focus-visible:outline-2 focus-visible:outline-ring group'
            onClick={() => setAdvancedOpen(!advancedOpen)}
            aria-expanded={advancedOpen}
            aria-controls={advancedId}
          >
            {advancedOpen ? (
              <ChevronDown className='h-3.5 w-3.5 text-foreground/50 shrink-0' aria-hidden='true' />
            ) : (
              <ChevronRight
                className='h-3.5 w-3.5 text-foreground/50 shrink-0'
                aria-hidden='true'
              />
            )}
            <SlidersHorizontal
              className='h-3.5 w-3.5 text-foreground/50 shrink-0'
              aria-hidden='true'
            />
            <span className='text-xs font-semibold uppercase tracking-[0.06em] text-foreground/60 group-hover:text-foreground/90 transition-colors'>
              Advanced
            </span>
          </button>
          {advancedOpen && (
            <div id={advancedId} className='mt-1 ml-[22px] border-l-2 border-border/40 pl-4 pb-3'>
              <OAuth2AdvancedSection
                oauth2={o}
                patchOAuth2={patchOAuth2}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          )}
        </section>
      )}

      {/* ── Additional Parameters (collapsible) ── */}
      <section className='border-t border-border/40'>
        <button
          type='button'
          className='flex w-full items-center gap-2 py-2.5 px-0.5 text-left rounded focus-visible:outline-2 focus-visible:outline-ring group'
          onClick={() => setAdditionalOpen(!additionalOpen)}
          aria-expanded={additionalOpen}
          aria-controls={additionalId}
        >
          {additionalOpen ? (
            <ChevronDown className='h-3.5 w-3.5 text-foreground/50 shrink-0' aria-hidden='true' />
          ) : (
            <ChevronRight className='h-3.5 w-3.5 text-foreground/50 shrink-0' aria-hidden='true' />
          )}
          <ListPlus className='h-3.5 w-3.5 text-foreground/50 shrink-0' aria-hidden='true' />
          <span className='text-xs font-semibold uppercase tracking-[0.06em] text-foreground/60 group-hover:text-foreground/90 transition-colors'>
            Additional Parameters
          </span>
        </button>
        {additionalOpen && (
          <div id={additionalId} className='mt-1 ml-[22px] border-l-2 border-border/40 pl-4 pb-3'>
            <OAuth2AdditionalParams
              oauth2={o}
              patchOAuth2={patchOAuth2}
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
        )}
      </section>

      {/* ── Settings ── */}
      <section aria-labelledby='oauth2-settings-heading' className='pt-1 border-t border-border/40'>
        <div className='flex items-center gap-2 py-2.5 mb-2'>
          <OAuth2SectionHeader icon={Settings2} title='Settings' />
        </div>
        <OAuth2SettingsSection oauth2={o} patchOAuth2={patchOAuth2} />
      </section>

      {/* ── Action bar ── */}
      <div className='flex items-center gap-2 pt-4 mt-2 border-t border-border/60'>
        <Button
          variant='default'
          size='sm'
          className='text-sm min-h-[36px]'
          disabled={getTokenDisabled}
          onClick={handleGetToken}
          aria-busy={gettingToken}
        >
          {gettingToken ? 'Waiting…' : 'Get Access Token'}
        </Button>
        {o.refreshToken && (
          <Button
            variant='outline'
            size='sm'
            className='text-sm min-h-[36px]'
            disabled={gettingToken || !o.tokenUrl}
            onClick={handleRefreshToken}
          >
            Refresh
          </Button>
        )}
        {hasAnyToken && (
          <Button
            variant='ghost'
            size='sm'
            className='ml-auto text-sm min-h-[36px] text-muted-foreground hover:text-foreground'
            disabled={gettingToken}
            onClick={handleClearCache}
          >
            Clear Cache
          </Button>
        )}
      </div>
      {tokenError && (
        <p
          role='alert'
          className='mt-2 rounded border border-destructive/40 bg-destructive/8 px-3 py-2 text-sm text-destructive leading-relaxed'
        >
          {tokenError}
        </p>
      )}
    </div>
  );
}

const GRANT_LABELS: Record<OAuth2GrantType, { hint: string }> = {
  client_credentials: { hint: 'Machine-to-machine — no user present' },
  password: { hint: 'Resource owner credentials — legacy' },
  authorization_code: { hint: 'User-delegated — recommended with PKCE' },
  implicit: { hint: 'Browser-only — deprecated' },
};
