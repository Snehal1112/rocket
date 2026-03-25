import { useCallback, useState } from 'react';
import { User, Lock, Key } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { AuthState } from '@/types/pane-types';
import { executeRequest, oauth2AuthCodeFlow } from '@/lib/tauri-api';

type AuthType = AuthState['authType'];
type OAuth2GrantType = NonNullable<AuthState['oauth2']>['grantType'];

interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
}

const AUTH_TYPES: { label: string; value: AuthType }[] = [
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
];

function tokenExpiryDisplay(expiresIn: number | null, acquiredAt: number | null): string {
  if (!expiresIn || !acquiredAt) return 'No expiry';
  const expiresAt = acquiredAt + expiresIn;
  const now = Math.floor(Date.now() / 1000);
  if (now >= expiresAt) return 'Expired';
  const remaining = expiresAt - now;
  const date = new Date(expiresAt * 1000);
  const time = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (remaining < 60) return `Expires in ${remaining}s (at ${time})`;
  if (remaining < 3600) return `Expires in ${Math.floor(remaining / 60)}m (at ${time})`;
  return `Expires in ${Math.floor(remaining / 3600)}h (at ${time})`;
}

export function AuthEditor({ auth, onChange }: AuthEditorProps) {
  const setType = useCallback(
    (authType: AuthType) => {
      const next: AuthState = { authType };
      if (authType === 'basic') next.basic = { username: '', password: '' };
      if (authType === 'bearer') next.bearer = { token: '' };
      if (authType === 'api-key')
        next.apiKey = { key: '', value: '', addTo: 'header' };
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
      onChange(next);
    },
    [onChange],
  );

  // Helper: patch oauth2 fields without losing other auth state.
  const patchOAuth2 = useCallback(
    (patch: Partial<NonNullable<AuthState['oauth2']>>) => {
      onChange({ ...auth, oauth2: { ...auth.oauth2!, ...patch } });
    },
    [auth, onChange],
  );

  const [tokenError, setTokenError] = useState('');
  const [gettingToken, setGettingToken] = useState(false);

  const handleGetToken = useCallback(async () => {
    const oauth = auth.oauth2;
    if (!oauth || !oauth.tokenUrl) return;
    if (oauth.grantType === 'authorization_code') {
      if (!oauth.authorizationUrl || !oauth.tokenUrl) return;
      setGettingToken(true);
      setTokenError('');
      try {
        const token = await oauth2AuthCodeFlow(
          oauth.authorizationUrl,
          oauth.tokenUrl,
          oauth.clientId,
          oauth.clientSecret,
          oauth.scope || undefined,
          oauth.callbackUrl || undefined,
        );
        patchOAuth2({
          accessToken: token.access_token,
          refreshToken: token.refresh_token || '',
          expiresIn: typeof token.expires_in === 'number' ? token.expires_in : null,
          tokenAcquiredAt: Math.floor(Date.now() / 1000),
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setTokenError(msg);
      } finally {
        setGettingToken(false);
      }
      return;
    }

    setTokenError('');
    const params = new URLSearchParams();
    params.set('grant_type', oauth.grantType);
    if (oauth.grantType === 'password') {
      params.set('username', oauth.username);
      params.set('password', oauth.password);
    }
    if (oauth.scope) params.set('scope', oauth.scope);

    const headers: { key: string; value: string; enabled: boolean }[] = [
      { key: 'Content-Type', value: 'application/x-www-form-urlencoded', enabled: true },
    ];
    if (oauth.clientAuthentication === 'header') {
      const basic = btoa(`${oauth.clientId}:${oauth.clientSecret}`);
      headers.push({ key: 'Authorization', value: `Basic ${basic}`, enabled: true });
    } else {
      params.set('client_id', oauth.clientId);
      params.set('client_secret', oauth.clientSecret);
    }

    try {
      const result = await executeRequest({
        method: 'POST',
        url: oauth.tokenUrl,
        headers,
        body: { mode: 'text', content: params.toString() },
        auth: { authType: 'none' },
        options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
      });

      const json = JSON.parse(result.body);
      if (json.error) {
        setTokenError(json.error_description || json.error);
        return;
      }

      // Validate the token is a non-empty string within a sane size bound.
      const MAX_TOKEN_BYTES = 8 * 1024;
      if (
        typeof json.access_token !== 'string' ||
        json.access_token.length === 0 ||
        json.access_token.length > MAX_TOKEN_BYTES
      ) {
        setTokenError('Server returned an invalid or missing access_token.');
        return;
      }

      const refreshToken =
        typeof json.refresh_token === 'string' &&
        json.refresh_token.length > 0 &&
        json.refresh_token.length <= MAX_TOKEN_BYTES
          ? json.refresh_token
          : '';

      patchOAuth2({
        accessToken: json.access_token,
        refreshToken,
        expiresIn: typeof json.expires_in === 'number' ? json.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setTokenError(msg);
      patchOAuth2({ accessToken: '' });
    }
  }, [auth.oauth2, patchOAuth2]);

  const handleRefreshToken = useCallback(async () => {
    const oauth = auth.oauth2;
    if (!oauth || !oauth.refreshToken || !oauth.tokenUrl) return;
    setGettingToken(true);
    setTokenError('');
    try {
      const params = new URLSearchParams();
      params.set('grant_type', 'refresh_token');
      params.set('refresh_token', oauth.refreshToken);
      const headers: { key: string; value: string; enabled: boolean }[] = [
        { key: 'Content-Type', value: 'application/x-www-form-urlencoded', enabled: true },
      ];
      if (oauth.clientAuthentication === 'header') {
        const basic = btoa(`${oauth.clientId}:${oauth.clientSecret}`);
        headers.push({ key: 'Authorization', value: `Basic ${basic}`, enabled: true });
      } else {
        params.set('client_id', oauth.clientId);
        params.set('client_secret', oauth.clientSecret);
      }
      if (oauth.scope) params.set('scope', oauth.scope);
      const result = await executeRequest({
        method: 'POST', url: oauth.tokenUrl, headers,
        body: { mode: 'text', content: params.toString() },
        auth: { authType: 'none' },
        options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
      });
      const json = JSON.parse(result.body);
      if (json.error) { setTokenError(json.error_description || json.error); return; }
      patchOAuth2({
        accessToken: json.access_token,
        refreshToken: json.refresh_token || oauth.refreshToken,
        expiresIn: typeof json.expires_in === 'number' ? json.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
      });
    } catch (err) {
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally { setGettingToken(false); }
  }, [auth.oauth2, patchOAuth2]);

  // Helper: patch awsSigV4 fields without losing other auth state.
  const patchAWS = useCallback(
    (patch: Partial<NonNullable<AuthState['awsSigV4']>>) => {
      onChange({ ...auth, awsSigV4: { ...auth.awsSigV4!, ...patch } });
    },
    [auth, onChange],
  );

  return (
    <div className="space-y-4">
      {/* Auth type selector. */}
      <Select value={auth.authType} onValueChange={(val) => setType(val as AuthType)}>
        <SelectTrigger className="w-[200px] h-8 text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {AUTH_TYPES.map((t) => (
            <SelectItem key={t.value} value={t.value} className="text-xs">
              {t.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* None: no fields shown. */}
      {auth.authType === 'none' && (
        <p className="text-xs text-muted-foreground">
          No authentication configured.
        </p>
      )}

      {/* Basic auth: icon + input rows. */}
      {auth.authType === 'basic' && auth.basic && (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <User className="h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Username"
              className="flex-1 text-xs h-8"
              value={auth.basic.username}
              onChange={(e) =>
                onChange({
                  ...auth,
                  basic: { ...auth.basic!, username: e.target.value },
                })
              }
            />
          </div>
          <div className="flex items-center gap-2">
            <Lock className="h-4 w-4 text-muted-foreground" />
            <Input
              type="password"
              placeholder="Password"
              className="flex-1 text-xs h-8"
              value={auth.basic.password}
              onChange={(e) =>
                onChange({
                  ...auth,
                  basic: { ...auth.basic!, password: e.target.value },
                })
              }
            />
          </div>
        </div>
      )}

      {/* Bearer: icon + token input. */}
      {auth.authType === 'bearer' && auth.bearer && (
        <div className="flex items-center gap-2">
          <Key className="h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Token"
            className="flex-1 text-xs h-8"
            value={auth.bearer.token}
            onChange={(e) =>
              onChange({
                ...auth,
                bearer: { token: e.target.value },
              })
            }
          />
        </div>
      )}

      {/* API Key: plain inputs + select. */}
      {auth.authType === 'api-key' && auth.apiKey && (
        <div className="space-y-2">
          <Input
            placeholder="Key"
            className="text-xs h-8"
            value={auth.apiKey.key}
            onChange={(e) =>
              onChange({
                ...auth,
                apiKey: { ...auth.apiKey!, key: e.target.value },
              })
            }
          />
          <Input
            placeholder="Value"
            className="text-xs h-8"
            value={auth.apiKey.value}
            onChange={(e) =>
              onChange({
                ...auth,
                apiKey: { ...auth.apiKey!, value: e.target.value },
              })
            }
          />
          <Select
            value={auth.apiKey.addTo}
            onValueChange={(val) =>
              onChange({
                ...auth,
                apiKey: {
                  ...auth.apiKey!,
                  addTo: val as 'header' | 'query',
                },
              })
            }
          >
            <SelectTrigger className="w-[120px] h-8 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="header" className="text-xs">Header</SelectItem>
              <SelectItem value="query" className="text-xs">Query Param</SelectItem>
            </SelectContent>
          </Select>
        </div>
      )}

      {/* OAuth 2.0: labeled fields with small labels. */}
      {auth.authType === 'oauth2' && auth.oauth2 && (() => {
        const o = auth.oauth2;
        return (
          <div className="space-y-3">
            {/* Grant Type. */}
            <div>
              <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Grant Type</label>
              <Select value={o.grantType} onValueChange={(val) => patchOAuth2({ grantType: val as OAuth2GrantType })}>
                <SelectTrigger className="w-[200px] h-8 text-xs"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="client_credentials" className="text-xs">Client Credentials</SelectItem>
                  <SelectItem value="password" className="text-xs">Password</SelectItem>
                  <SelectItem value="authorization_code" className="text-xs">Authorization Code</SelectItem>
                  <SelectItem value="implicit" className="text-xs">Implicit</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Authorization URL — auth code and implicit only. */}
            {(o.grantType === 'authorization_code' || o.grantType === 'implicit') && (
              <div>
                <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Authorization URL</label>
                <Input className="text-xs h-8 font-mono" placeholder="https://auth.example.com/authorize" value={o.authorizationUrl} onChange={(e) => patchOAuth2({ authorizationUrl: e.target.value })} />
              </div>
            )}

            {/* Token URL — hidden for implicit. */}
            {o.grantType !== 'implicit' && (
              <div>
                <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Token URL</label>
                <Input className="text-xs h-8 font-mono" placeholder="https://auth.example.com/token" value={o.tokenUrl} onChange={(e) => patchOAuth2({ tokenUrl: e.target.value })} />
              </div>
            )}

            {/* Callback URL + State — auth code and implicit only. */}
            {(o.grantType === 'authorization_code' || o.grantType === 'implicit') && (<>
              <div>
                <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Callback URL</label>
                <div className="flex gap-1.5">
                  <Input className="text-xs h-8 font-mono flex-1" value={o.callbackUrl} onChange={(e) => patchOAuth2({ callbackUrl: e.target.value })} />
                  <Button variant="outline" size="sm" className="h-8 px-2 text-xs shrink-0" onClick={() => navigator.clipboard.writeText(o.callbackUrl)} title="Copy">Copy</Button>
                </div>
              </div>
              <div>
                <label className="text-[11px] font-medium text-muted-foreground mb-1 block">State</label>
                <Input className="text-xs h-8" placeholder="Leave empty for auto-generated" value={o.state} onChange={(e) => patchOAuth2({ state: e.target.value })} />
              </div>
            </>)}

            {/* Client ID + Secret — Secret hidden for implicit. */}
            <div className={o.grantType === 'implicit' ? '' : 'grid grid-cols-2 gap-2'}>
              <div>
                <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Client ID</label>
                <Input className="text-xs h-8" placeholder="client-id" value={o.clientId} onChange={(e) => patchOAuth2({ clientId: e.target.value })} />
              </div>
              {o.grantType !== 'implicit' && (
                <div>
                  <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Client Secret</label>
                  <Input className="text-xs h-8" type="password" placeholder="client-secret" value={o.clientSecret} onChange={(e) => patchOAuth2({ clientSecret: e.target.value })} />
                </div>
              )}
            </div>

            {/* Scope — always visible. */}
            <div>
              <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Scope</label>
              <Input className="text-xs h-8" placeholder="read write" value={o.scope} onChange={(e) => patchOAuth2({ scope: e.target.value })} />
            </div>

            {/* Username + Password — password grant only. */}
            {o.grantType === 'password' && (
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Username</label>
                  <Input className="text-xs h-8" placeholder="user@example.com" value={o.username} onChange={(e) => patchOAuth2({ username: e.target.value })} />
                </div>
                <div>
                  <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Password</label>
                  <Input className="text-xs h-8" type="password" value={o.password} onChange={(e) => patchOAuth2({ password: e.target.value })} />
                </div>
              </div>
            )}

            {/* Advanced Options — collapsible. */}
            <details className="text-xs">
              <summary className="cursor-pointer text-muted-foreground hover:text-foreground py-1">Advanced Options</summary>
              <div className="space-y-3 mt-2 pl-1">
                <div>
                  <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Client Authentication</label>
                  <Select value={o.clientAuthentication} onValueChange={(v) => patchOAuth2({ clientAuthentication: v as 'header' | 'body' })}>
                    <SelectTrigger className="w-full h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="body" className="text-xs">Send in Request Body</SelectItem>
                      <SelectItem value="header" className="text-xs">Send as Basic Auth Header</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div>
                  <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Header Prefix</label>
                  <Input className="text-xs h-8" value={o.headerPrefix} onChange={(e) => patchOAuth2({ headerPrefix: e.target.value })} />
                </div>
                <div>
                  <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Add Token To</label>
                  <Select value={o.addTokenTo} onValueChange={(v) => patchOAuth2({ addTokenTo: v as 'header' | 'queryParams' })}>
                    <SelectTrigger className="w-full h-8 text-xs"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="header" className="text-xs">Header</SelectItem>
                      <SelectItem value="queryParams" className="text-xs">Query Params</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </details>

            {/* Token section. */}
            <div className="space-y-2 border-t border-border/50 pt-3 mt-3">
              <div>
                <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Access Token</label>
                <div className="flex gap-1.5">
                  <Input className="h-8 flex-1 text-xs truncate" readOnly value={o.accessToken} placeholder="(none)" title={o.accessToken || undefined} />
                  <Button variant="outline" size="sm" className="h-8 px-2 text-xs shrink-0" onClick={() => navigator.clipboard.writeText(o.accessToken)} title="Copy">Copy</Button>
                </div>
              </div>
              {o.refreshToken && (
                <div>
                  <label className="text-[11px] font-medium text-muted-foreground mb-1 block">Refresh Token</label>
                  <div className="flex gap-1.5">
                    <Input className="h-8 flex-1 text-xs truncate" readOnly value={o.refreshToken} />
                    <Button variant="outline" size="sm" className="h-8 px-2 text-xs shrink-0" onClick={() => navigator.clipboard.writeText(o.refreshToken)} title="Copy">Copy</Button>
                  </div>
                </div>
              )}
              <p className="text-[10px] text-muted-foreground">{tokenExpiryDisplay(o.expiresIn, o.tokenAcquiredAt)}</p>
              <div className="flex gap-2">
                <Button variant="outline" size="sm" className="h-8 text-xs" disabled={gettingToken || (!o.tokenUrl && o.grantType !== 'implicit')} onClick={handleGetToken}>
                  {gettingToken ? 'Waiting...' : 'Get Token'}
                </Button>
                {o.refreshToken && (
                  <Button variant="outline" size="sm" className="h-8 text-xs" disabled={gettingToken || !o.tokenUrl} onClick={handleRefreshToken}>
                    Refresh
                  </Button>
                )}
              </div>
              {tokenError && <p className="text-[10px] text-destructive">{tokenError}</p>}
            </div>
          </div>
        );
      })()}

      {/* AWS Signature v4 panel. */}
      {auth.authType === 'aws-sig-v4' && auth.awsSigV4 && (
        <div className="space-y-3">
          <div>
            <label className="text-[11px] font-medium text-muted-foreground mb-1 block">
              Access Key
            </label>
            <Input
              className="text-xs h-8"
              placeholder="AKIAIOSFODNN7EXAMPLE"
              value={auth.awsSigV4.accessKey}
              onChange={(e) => patchAWS({ accessKey: e.target.value })}
            />
          </div>

          <div>
            <label className="text-[11px] font-medium text-muted-foreground mb-1 block">
              Secret Key
            </label>
            <Input
              className="text-xs h-8"
              type="password"
              placeholder="wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
              value={auth.awsSigV4.secretKey}
              onChange={(e) => patchAWS({ secretKey: e.target.value })}
            />
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="text-[11px] font-medium text-muted-foreground mb-1 block">
                Region
              </label>
              <Input
                className="text-xs h-8"
                placeholder="us-east-1"
                value={auth.awsSigV4.region}
                onChange={(e) => patchAWS({ region: e.target.value })}
              />
            </div>
            <div>
              <label className="text-[11px] font-medium text-muted-foreground mb-1 block">
                Service
              </label>
              <Input
                className="text-xs h-8"
                placeholder="execute-api"
                value={auth.awsSigV4.service}
                onChange={(e) => patchAWS({ service: e.target.value })}
              />
            </div>
          </div>

          <div>
            <label className="text-[11px] font-medium text-muted-foreground mb-1 block">
              Session Token
            </label>
            <Input
              className="text-xs h-8"
              type="password"
              placeholder="(optional)"
              value={auth.awsSigV4.sessionToken}
              onChange={(e) => patchAWS({ sessionToken: e.target.value })}
            />
            <p className="mt-1 text-[11px] text-muted-foreground">
              Session token is only required for temporary credentials.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
