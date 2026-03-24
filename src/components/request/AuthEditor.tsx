import { useCallback } from 'react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { AuthState } from '@/types/pane-types';

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

// Two-column grid row: label on the left, input on the right.
function FieldRow({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[7rem_1fr] items-center gap-2">
      <Label htmlFor={htmlFor} className="text-sm text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  );
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
          clientId: '',
          clientSecret: '',
          tokenUrl: '',
          scope: '',
          accessToken: '',
          refreshToken: '',
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

  // Helper: patch awsSigV4 fields without losing other auth state.
  const patchAWS = useCallback(
    (patch: Partial<NonNullable<AuthState['awsSigV4']>>) => {
      onChange({ ...auth, awsSigV4: { ...auth.awsSigV4!, ...patch } });
    },
    [auth, onChange],
  );

  return (
    <div className="space-y-3">
      {/* Auth type selector using shadcn Select. */}
      <div className="max-w-xs">
        <Label className="mb-1 block text-xs text-muted-foreground">Auth Type</Label>
        <Select value={auth.authType} onValueChange={(val) => setType(val as AuthType)}>
          <SelectTrigger className="h-8 text-xs">
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
      </div>

      {/* Auth fields. */}
      {auth.authType === 'none' && (
        <p className="text-xs text-muted-foreground">
          No authentication configured.
        </p>
      )}

      {auth.authType === 'basic' && auth.basic && (
        <div className="space-y-2">
          <div>
            <Label htmlFor="auth-basic-username" className="text-xs text-muted-foreground">
              Username
            </Label>
            <Input
              id="auth-basic-username"
              className="mt-1 h-7 text-xs"
              value={auth.basic.username}
              onChange={(e) =>
                onChange({
                  ...auth,
                  basic: { ...auth.basic!, username: e.target.value },
                })
              }
            />
          </div>
          <div>
            <Label htmlFor="auth-basic-password" className="text-xs text-muted-foreground">
              Password
            </Label>
            <Input
              id="auth-basic-password"
              className="mt-1 h-7 text-xs"
              type="password"
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

      {auth.authType === 'bearer' && auth.bearer && (
        <div>
          <Label htmlFor="auth-bearer-token" className="text-xs text-muted-foreground">
            Token
          </Label>
          <Input
            id="auth-bearer-token"
            className="mt-1 h-7 text-xs"
            placeholder="Bearer token"
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

      {auth.authType === 'api-key' && auth.apiKey && (
        <div className="space-y-2">
          <div>
            <Label htmlFor="auth-apikey-key" className="text-xs text-muted-foreground">
              Key
            </Label>
            <Input
              id="auth-apikey-key"
              className="mt-1 h-7 text-xs"
              placeholder="X-API-Key"
              value={auth.apiKey.key}
              onChange={(e) =>
                onChange({
                  ...auth,
                  apiKey: { ...auth.apiKey!, key: e.target.value },
                })
              }
            />
          </div>
          <div>
            <Label htmlFor="auth-apikey-value" className="text-xs text-muted-foreground">
              Value
            </Label>
            <Input
              id="auth-apikey-value"
              className="mt-1 h-7 text-xs"
              placeholder="api-key-value"
              value={auth.apiKey.value}
              onChange={(e) =>
                onChange({
                  ...auth,
                  apiKey: { ...auth.apiKey!, value: e.target.value },
                })
              }
            />
          </div>
          <div>
            <Label className="text-xs text-muted-foreground">Add to</Label>
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
              <SelectTrigger className="mt-1 h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="header" className="text-xs">Header</SelectItem>
                <SelectItem value="query" className="text-xs">Query Param</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      )}

      {/* OAuth 2.0 panel. */}
      {auth.authType === 'oauth2' && auth.oauth2 && (
        <div className="space-y-2">
          <FieldRow label="Grant type">
            <Select
              value={auth.oauth2.grantType}
              onValueChange={(val) =>
                patchOAuth2({ grantType: val as OAuth2GrantType })
              }
            >
              <SelectTrigger className="h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="client_credentials" className="text-xs">Client Credentials</SelectItem>
                <SelectItem value="password" className="text-xs">Password</SelectItem>
                <SelectItem value="authorization_code" className="text-xs">Authorization Code</SelectItem>
              </SelectContent>
            </Select>
          </FieldRow>

          <FieldRow label="Client ID">
            <Input
              className="h-7 text-xs"
              placeholder="client-id"
              value={auth.oauth2.clientId}
              onChange={(e) => patchOAuth2({ clientId: e.target.value })}
            />
          </FieldRow>

          <FieldRow label="Client Secret">
            <Input
              className="h-7 text-xs"
              type="password"
              placeholder="client-secret"
              value={auth.oauth2.clientSecret}
              onChange={(e) => patchOAuth2({ clientSecret: e.target.value })}
            />
          </FieldRow>

          <FieldRow label="Token URL">
            <Input
              className="h-7 text-xs"
              placeholder="https://auth.example.com/token"
              value={auth.oauth2.tokenUrl}
              onChange={(e) => patchOAuth2({ tokenUrl: e.target.value })}
            />
          </FieldRow>

          <FieldRow label="Scope">
            <Input
              className="h-7 text-xs"
              placeholder="read write"
              value={auth.oauth2.scope}
              onChange={(e) => patchOAuth2({ scope: e.target.value })}
            />
          </FieldRow>

          {/* Access token row with "Get Token" action. */}
          <FieldRow label="Access token">
            <div className="flex items-center gap-1.5">
              <Input
                className="h-7 flex-1 truncate text-xs"
                readOnly
                placeholder="(none)"
                value={auth.oauth2.accessToken}
                title={auth.oauth2.accessToken || undefined}
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="h-7 shrink-0 px-2 text-xs"
                // Actual fetch wired up via Tauri invoke in a later task.
                onClick={() => {}}
              >
                Get Token
              </Button>
            </div>
          </FieldRow>

          {auth.oauth2.refreshToken && (
            <FieldRow label="Refresh token">
              <Input
                className="h-7 truncate text-xs"
                readOnly
                value={auth.oauth2.refreshToken}
                title={auth.oauth2.refreshToken}
              />
            </FieldRow>
          )}
        </div>
      )}

      {/* AWS Signature v4 panel. */}
      {auth.authType === 'aws-sig-v4' && auth.awsSigV4 && (
        <div className="space-y-2">
          <FieldRow label="Access Key">
            <Input
              className="h-7 text-xs"
              placeholder="AKIAIOSFODNN7EXAMPLE"
              value={auth.awsSigV4.accessKey}
              onChange={(e) => patchAWS({ accessKey: e.target.value })}
            />
          </FieldRow>

          <FieldRow label="Secret Key">
            <Input
              className="h-7 text-xs"
              type="password"
              placeholder="wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
              value={auth.awsSigV4.secretKey}
              onChange={(e) => patchAWS({ secretKey: e.target.value })}
            />
          </FieldRow>

          <FieldRow label="Region">
            <Input
              className="h-7 text-xs"
              placeholder="us-east-1"
              value={auth.awsSigV4.region}
              onChange={(e) => patchAWS({ region: e.target.value })}
            />
          </FieldRow>

          <FieldRow label="Service">
            <Input
              className="h-7 text-xs"
              placeholder="execute-api"
              value={auth.awsSigV4.service}
              onChange={(e) => patchAWS({ service: e.target.value })}
            />
          </FieldRow>

          <FieldRow label="Session Token">
            <Input
              className="h-7 text-xs"
              type="password"
              placeholder="(optional)"
              value={auth.awsSigV4.sessionToken}
              onChange={(e) => patchAWS({ sessionToken: e.target.value })}
            />
          </FieldRow>
          <p className="pl-[7.5rem] text-xs text-muted-foreground">
            Session token is only required for temporary credentials.
          </p>
        </div>
      )}
    </div>
  );
}
