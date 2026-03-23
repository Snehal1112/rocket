import { useCallback } from 'react';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
import type { AuthState } from '@/types/pane-types';

type AuthType = AuthState['authType'];

interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
}

const AUTH_TYPES: { label: string; value: AuthType }[] = [
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
];

export function AuthEditor({ auth, onChange }: AuthEditorProps) {
  const setType = useCallback(
    (authType: AuthType) => {
      const next: AuthState = { authType };
      if (authType === 'basic') next.basic = { username: '', password: '' };
      if (authType === 'bearer') next.bearer = { token: '' };
      if (authType === 'api-key')
        next.apiKey = { key: '', value: '', addTo: 'header' };
      onChange(next);
    },
    [onChange],
  );

  return (
    <div className="space-y-3">
      {/* Type selector. */}
      <div className="flex gap-1 border-b border-border pb-1">
        {AUTH_TYPES.map((t) => (
          <button
            key={t.value}
            type="button"
            className={cn(
              'rounded-md px-2.5 py-1 text-xs font-medium transition-colors',
              auth.authType === t.value
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground',
            )}
            onClick={() => setType(t.value)}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Auth fields. */}
      {auth.authType === 'none' && (
        <p className="text-xs text-muted-foreground">
          No authentication configured.
        </p>
      )}

      {auth.authType === 'basic' && auth.basic && (
        <div className="space-y-2">
          <label className="block text-xs font-medium text-muted-foreground">
            Username
            <Input
              className="mt-1 h-7 text-xs"
              value={auth.basic.username}
              onChange={(e) =>
                onChange({
                  ...auth,
                  basic: { ...auth.basic!, username: e.target.value },
                })
              }
            />
          </label>
          <label className="block text-xs font-medium text-muted-foreground">
            Password
            <Input
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
          </label>
        </div>
      )}

      {auth.authType === 'bearer' && auth.bearer && (
        <label className="block text-xs font-medium text-muted-foreground">
          Token
          <Input
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
        </label>
      )}

      {auth.authType === 'api-key' && auth.apiKey && (
        <div className="space-y-2">
          <label className="block text-xs font-medium text-muted-foreground">
            Key
            <Input
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
          </label>
          <label className="block text-xs font-medium text-muted-foreground">
            Value
            <Input
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
          </label>
          <label className="block text-xs font-medium text-muted-foreground">
            Add to
            <select
              className="mt-1 block h-7 w-full rounded-md border border-input bg-transparent px-2 text-xs shadow-xs outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
              value={auth.apiKey.addTo}
              onChange={(e) =>
                onChange({
                  ...auth,
                  apiKey: {
                    ...auth.apiKey!,
                    addTo: e.target.value as 'header' | 'query',
                  },
                })
              }
            >
              <option value="header">Header</option>
              <option value="query">Query Param</option>
            </select>
          </label>
        </div>
      )}
    </div>
  );
}
