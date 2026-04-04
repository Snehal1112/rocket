import { ChevronDown, ChevronRight, Key, Lock, User } from 'lucide-react';
import { useCallback, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { executeRequest, oauth2AuthCodeFlow } from '@/lib/tauri-api';
import type { AuthState } from '@/types/pane-types';

type OAuth2GrantType = NonNullable<AuthState['oauth2']>['grantType'];

interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
}

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
  // Helper: patch oauth2 fields without losing other auth state.
  const patchOAuth2 = useCallback(
    (patch: Partial<NonNullable<AuthState['oauth2']>>) => {
      onChange({ ...auth, oauth2: { ...auth.oauth2!, ...patch } });
    },
    [auth, onChange],
  );

  const [tokenError, setTokenError] = useState('');
  const [gettingToken, setGettingToken] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const handleGetToken = useCallback(async () => {
    const oauth = auth.oauth2;
    if (!oauth?.tokenUrl) return;
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
          oauth.verifySsl,
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
        queryParams: [],
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
    if (!oauth?.refreshToken || !oauth?.tokenUrl) return;
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
        method: 'POST',
        url: oauth.tokenUrl,
        headers,
        queryParams: [],
        body: { mode: 'text', content: params.toString() },
        auth: { authType: 'none' },
        options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
      });
      const json = JSON.parse(result.body);
      if (json.error) {
        setTokenError(json.error_description || json.error);
        return;
      }
      patchOAuth2({
        accessToken: json.access_token,
        refreshToken: json.refresh_token || oauth.refreshToken,
        expiresIn: typeof json.expires_in === 'number' ? json.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
      });
    } catch (err) {
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally {
      setGettingToken(false);
    }
  }, [auth.oauth2, patchOAuth2]);

  // Helper: patch awsSigV4 fields without losing other auth state.
  const patchAWS = useCallback(
    (patch: Partial<NonNullable<AuthState['awsSigV4']>>) => {
      onChange({ ...auth, awsSigV4: { ...auth.awsSigV4!, ...patch } });
    },
    [auth, onChange],
  );

  return (
    <div className='space-y-4'>
      {/* Inherit: shows message about collection auth. */}
      {auth.authType === 'inherit' && (
        <div className='rounded-md border border-border bg-muted/50 px-3 py-2.5'>
          <p className='text-xs text-muted-foreground'>
            This request inherits authorization from the collection settings. To override, select a
            different auth type above.
          </p>
        </div>
      )}

      {/* None: no fields shown. */}
      {auth.authType === 'none' && (
        <p className='text-xs text-muted-foreground'>No authentication configured.</p>
      )}

      {/* Basic auth: icon + input rows. */}
      {auth.authType === 'basic' && auth.basic && (
        <div className='space-y-2'>
          <div className='flex items-center gap-2'>
            <User className='h-3.5 w-3.5 text-muted-foreground' />
            <Input
              placeholder='Username'
              className='flex-1 text-sm'
              value={auth.basic.username}
              onChange={(e) =>
                onChange({
                  ...auth,
                  basic: { ...auth.basic!, username: e.target.value },
                })
              }
            />
          </div>
          <div className='flex items-center gap-2'>
            <Lock className='h-3.5 w-3.5 text-muted-foreground' />
            <Input
              type='password'
              placeholder='Password'
              className='flex-1 text-sm'
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
        <div className='flex items-center gap-2'>
          <Key className='h-3.5 w-3.5 text-muted-foreground' />
          <Input
            placeholder='Token'
            className='flex-1 text-sm'
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
        <div className='space-y-2'>
          <Input
            placeholder='Key'
            className='text-sm'
            value={auth.apiKey.key}
            onChange={(e) =>
              onChange({
                ...auth,
                apiKey: { ...auth.apiKey!, key: e.target.value },
              })
            }
          />
          <Input
            placeholder='Value'
            className='text-sm'
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
            <SelectTrigger className='w-[120px] text-sm'>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value='header' className='text-sm'>
                Header
              </SelectItem>
              <SelectItem value='query' className='text-sm'>
                Query Param
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
      )}

      {/* OAuth 2.0: labeled fields with small labels. */}
      {auth.authType === 'oauth2' &&
        auth.oauth2 &&
        (() => {
          const o = auth.oauth2;
          return (
            <div className='space-y-3'>
              {/* Grant Type. */}
              <div>
                <Label className='mb-1 block'>Grant Type</Label>
                <Select
                  value={o.grantType}
                  onValueChange={(val) => patchOAuth2({ grantType: val as OAuth2GrantType })}
                >
                  <SelectTrigger className='w-[200px] text-sm'>
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

              {/* Authorization URL — auth code and implicit only. */}
              {(o.grantType === 'authorization_code' || o.grantType === 'implicit') && (
                <div>
                  <Label className='mb-1 block'>Authorization URL</Label>
                  <Input
                    className='text-sm font-mono'
                    placeholder='https://auth.example.com/authorize'
                    value={o.authorizationUrl}
                    onChange={(e) => patchOAuth2({ authorizationUrl: e.target.value })}
                  />
                </div>
              )}

              {/* Token URL — hidden for implicit. */}
              {o.grantType !== 'implicit' && (
                <div>
                  <Label className='mb-1 block'>Token URL</Label>
                  <Input
                    className='text-sm font-mono'
                    placeholder='https://auth.example.com/token'
                    value={o.tokenUrl}
                    onChange={(e) => patchOAuth2({ tokenUrl: e.target.value })}
                  />
                </div>
              )}

              {/* Callback URL + State — auth code and implicit only. */}
              {(o.grantType === 'authorization_code' || o.grantType === 'implicit') && (
                <>
                  <div>
                    <Label className='mb-1 block'>Callback URL</Label>
                    <div className='flex gap-1.5'>
                      <Input
                        className='text-sm font-mono flex-1'
                        value={o.callbackUrl}
                        onChange={(e) => patchOAuth2({ callbackUrl: e.target.value })}
                      />
                      <Button
                        variant='outline'
                        size='sm'
                        className='px-2 text-sm shrink-0'
                        onClick={() => navigator.clipboard.writeText(o.callbackUrl)}
                        title='Copy'
                      >
                        Copy
                      </Button>
                    </div>
                  </div>
                  <div>
                    <Label className='mb-1 block'>State</Label>
                    <Input
                      className='text-sm'
                      placeholder='Leave empty for auto-generated'
                      value={o.state}
                      onChange={(e) => patchOAuth2({ state: e.target.value })}
                    />
                  </div>
                </>
              )}

              {/* Client ID + Secret — Secret hidden for implicit. */}
              <div className={o.grantType === 'implicit' ? '' : 'grid grid-cols-2 gap-2'}>
                <div>
                  <Label className='mb-1 block'>Client ID</Label>
                  <Input
                    className='text-sm'
                    placeholder='client-id'
                    value={o.clientId}
                    onChange={(e) => patchOAuth2({ clientId: e.target.value })}
                  />
                </div>
                {o.grantType !== 'implicit' && (
                  <div>
                    <Label className='mb-1 block'>Client Secret</Label>
                    <Input
                      className='text-sm'
                      type='password'
                      placeholder='client-secret'
                      value={o.clientSecret}
                      onChange={(e) => patchOAuth2({ clientSecret: e.target.value })}
                    />
                  </div>
                )}
              </div>

              {/* Scope — always visible. */}
              <div>
                <Label className='mb-1 block'>Scope</Label>
                <Input
                  className='text-sm'
                  placeholder='read write'
                  value={o.scope}
                  onChange={(e) => patchOAuth2({ scope: e.target.value })}
                />
              </div>

              {/* Username + Password — password grant only. */}
              {o.grantType === 'password' && (
                <div className='grid grid-cols-2 gap-2'>
                  <div>
                    <Label className='mb-1 block'>Username</Label>
                    <Input
                      className='text-sm'
                      placeholder='user@example.com'
                      value={o.username}
                      onChange={(e) => patchOAuth2({ username: e.target.value })}
                    />
                  </div>
                  <div>
                    <Label className='mb-1 block'>Password</Label>
                    <Input
                      className='text-sm'
                      type='password'
                      value={o.password}
                      onChange={(e) => patchOAuth2({ password: e.target.value })}
                    />
                  </div>
                </div>
              )}

              {/* Advanced Options — collapsible. */}
              <div className='text-sm'>
                <button
                  type='button'
                  className='flex items-center gap-1 cursor-pointer text-muted-foreground hover:text-foreground py-1'
                  onClick={() => setAdvancedOpen(!advancedOpen)}
                >
                  {advancedOpen ? (
                    <ChevronDown className='h-3.5 w-3.5' />
                  ) : (
                    <ChevronRight className='h-3.5 w-3.5' />
                  )}
                  Advanced Options
                </button>
                {advancedOpen && (
                  <div className='space-y-3 mt-2 pl-1'>
                    <div>
                      <Label className='mb-1 block'>Client Authentication</Label>
                      <Select
                        value={o.clientAuthentication}
                        onValueChange={(v) =>
                          patchOAuth2({ clientAuthentication: v as 'header' | 'body' })
                        }
                      >
                        <SelectTrigger className='w-full text-sm'>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value='body' className='text-sm'>
                            Send in Request Body
                          </SelectItem>
                          <SelectItem value='header' className='text-sm'>
                            Send as Basic Auth Header
                          </SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                    <div>
                      <Label className='mb-1 block'>Header Prefix</Label>
                      <Input
                        className='text-sm'
                        value={o.headerPrefix}
                        onChange={(e) => patchOAuth2({ headerPrefix: e.target.value })}
                      />
                    </div>
                    <div>
                      <Label className='mb-1 block'>Add Token To</Label>
                      <Select
                        value={o.addTokenTo}
                        onValueChange={(v) =>
                          patchOAuth2({ addTokenTo: v as 'header' | 'queryParams' })
                        }
                      >
                        <SelectTrigger className='w-full text-sm'>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value='header' className='text-sm'>
                            Header
                          </SelectItem>
                          <SelectItem value='queryParams' className='text-sm'>
                            Query Params
                          </SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                    <div className='flex items-center gap-2'>
                      <Checkbox
                        id='verify-ssl'
                        checked={o.verifySsl}
                        onCheckedChange={(checked) => patchOAuth2({ verifySsl: !!checked })}
                      />
                      <Label
                        htmlFor='verify-ssl'
                        className='text-xs text-muted-foreground cursor-pointer'
                      >
                        Verify SSL certificates
                      </Label>
                    </div>
                  </div>
                )}
              </div>

              {/* Token section. */}
              <div className='space-y-2 border-t border-border/50 pt-3 mt-3'>
                <div>
                  <Label className='mb-1 block'>Access Token</Label>
                  <div className='flex gap-1.5'>
                    <Input
                      className='flex-1 text-sm truncate'
                      readOnly
                      value={o.accessToken}
                      placeholder='(none)'
                      title={o.accessToken || undefined}
                    />
                    <Button
                      variant='outline'
                      size='sm'
                      className='px-2 text-sm shrink-0'
                      onClick={() => navigator.clipboard.writeText(o.accessToken)}
                      title='Copy'
                    >
                      Copy
                    </Button>
                  </div>
                </div>
                {o.refreshToken && (
                  <div>
                    <Label className='mb-1 block'>Refresh Token</Label>
                    <div className='flex gap-1.5'>
                      <Input className='flex-1 text-sm truncate' readOnly value={o.refreshToken} />
                      <Button
                        variant='outline'
                        size='sm'
                        className='px-2 text-sm shrink-0'
                        onClick={() => navigator.clipboard.writeText(o.refreshToken)}
                        title='Copy'
                      >
                        Copy
                      </Button>
                    </div>
                  </div>
                )}
                <p className='text-2xs text-muted-foreground'>
                  {tokenExpiryDisplay(o.expiresIn, o.tokenAcquiredAt)}
                </p>
                <div className='flex gap-2'>
                  <Button
                    variant='outline'
                    size='sm'
                    className='text-sm'
                    disabled={gettingToken || (!o.tokenUrl && o.grantType !== 'implicit')}
                    onClick={handleGetToken}
                  >
                    {gettingToken ? 'Waiting...' : 'Get Token'}
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
                </div>
                {tokenError && <p className='text-xs text-destructive'>{tokenError}</p>}
              </div>
            </div>
          );
        })()}

      {/* AWS Signature v4 panel. */}
      {auth.authType === 'aws-sig-v4' && auth.awsSigV4 && (
        <div className='space-y-3'>
          <div>
            <Label className='mb-1 block'>Access Key</Label>
            <Input
              className='text-sm'
              placeholder='AKIAIOSFODNN7EXAMPLE'
              value={auth.awsSigV4.accessKey}
              onChange={(e) => patchAWS({ accessKey: e.target.value })}
            />
          </div>

          <div>
            <Label className='mb-1 block'>Secret Key</Label>
            <Input
              className='text-sm'
              type='password'
              placeholder='wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY'
              value={auth.awsSigV4.secretKey}
              onChange={(e) => patchAWS({ secretKey: e.target.value })}
            />
          </div>

          <div className='grid grid-cols-2 gap-2'>
            <div>
              <Label className='mb-1 block'>Region</Label>
              <Input
                className='text-sm'
                placeholder='us-east-1'
                value={auth.awsSigV4.region}
                onChange={(e) => patchAWS({ region: e.target.value })}
              />
            </div>
            <div>
              <Label className='mb-1 block'>Service</Label>
              <Input
                className='text-sm'
                placeholder='execute-api'
                value={auth.awsSigV4.service}
                onChange={(e) => patchAWS({ service: e.target.value })}
              />
            </div>
          </div>

          <div>
            <Label className='mb-1 block'>Session Token</Label>
            <Input
              className='text-sm'
              type='password'
              placeholder='(optional)'
              value={auth.awsSigV4.sessionToken}
              onChange={(e) => patchAWS({ sessionToken: e.target.value })}
            />
            <p className='mt-1 text-xs text-muted-foreground'>
              Session token is only required for temporary credentials.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
