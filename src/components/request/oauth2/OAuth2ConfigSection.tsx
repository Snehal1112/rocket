import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface OAuth2ConfigSectionProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function OAuth2ConfigSection({
  oauth2: o,
  patchOAuth2,
  variableContext,
  onNavigateToSource,
}: OAuth2ConfigSectionProps) {
  const showAuthUrl = o.grantType === 'authorization_code' || o.grantType === 'implicit';
  const showTokenUrl = o.grantType !== 'implicit';
  const showCallback = showAuthUrl;
  const showSecret = o.grantType !== 'implicit';
  const showState = showAuthUrl;
  const showClientAuth = o.grantType !== 'implicit';
  const showPkce = o.grantType === 'authorization_code';
  const showSystemBrowser = o.grantType === 'authorization_code';
  const showResourceOwner = o.grantType === 'password';

  const hasEndpoints = showAuthUrl || showTokenUrl || showCallback;

  return (
    <div className='space-y-4'>
      {/* Endpoints group — URLs the flow talks to. */}
      {hasEndpoints && (
        <div className='space-y-3'>
          {showAuthUrl && (
            <div>
              <Label className='mb-1.5 block text-sm font-medium'>Authorization URL</Label>
              <SingleLineEditor
                className='text-sm font-mono'
                placeholder='https://auth.example.com/authorize'
                value={o.authorizationUrl}
                onChange={(newVal) => patchOAuth2({ authorizationUrl: newVal })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          )}

          {showTokenUrl && (
            <div>
              <Label className='mb-1.5 block text-sm font-medium'>Access Token URL</Label>
              <SingleLineEditor
                className='text-sm font-mono'
                placeholder='https://auth.example.com/token'
                value={o.tokenUrl}
                onChange={(newVal) => patchOAuth2({ tokenUrl: newVal })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          )}

          {showCallback && (
            <div>
              <Label className='mb-1.5 block text-sm font-medium'>Callback URL</Label>
              <div className='flex gap-2'>
                <SingleLineEditor
                  className='text-sm font-mono flex-1'
                  value={o.callbackUrl}
                  onChange={(newVal) => patchOAuth2({ callbackUrl: newVal })}
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
                <Button
                  variant='outline'
                  size='sm'
                  className='px-3 text-sm shrink-0'
                  onClick={() => navigator.clipboard.writeText(o.callbackUrl)}
                  aria-label='Copy callback URL to clipboard'
                >
                  Copy
                </Button>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Credentials group — identity and secret. */}
      <div className='space-y-3'>
        <div className={showSecret ? 'grid grid-cols-2 gap-3' : ''}>
          <div>
            <Label className='mb-1.5 block text-sm font-medium'>Client ID</Label>
            <SingleLineEditor
              className='text-sm'
              placeholder='client-id'
              value={o.clientId}
              onChange={(newVal) => patchOAuth2({ clientId: newVal })}
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
          {showSecret && (
            <div>
              <Label className='mb-1.5 block text-sm font-medium'>Client Secret</Label>
              <SingleLineEditor
                className='text-sm'
                isSecret
                placeholder='client-secret'
                value={o.clientSecret}
                onChange={(newVal) => patchOAuth2({ clientSecret: newVal })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          )}
        </div>

        {showResourceOwner && (
          <div className='grid grid-cols-2 gap-3'>
            <div>
              <Label className='mb-1.5 block text-sm font-medium'>Username</Label>
              <SingleLineEditor
                className='text-sm'
                placeholder='user@example.com'
                value={o.username}
                onChange={(newVal) => patchOAuth2({ username: newVal })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
            <div>
              <Label className='mb-1.5 block text-sm font-medium'>Password</Label>
              <SingleLineEditor
                className='text-sm'
                isSecret
                value={o.password}
                onChange={(newVal) => patchOAuth2({ password: newVal })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          </div>
        )}

        {showClientAuth && (
          <div>
            <Label className='mb-1.5 block text-sm font-medium'>Send Credentials</Label>
            <Select
              value={o.clientAuthentication}
              onValueChange={(v) => patchOAuth2({ clientAuthentication: v as 'header' | 'body' })}
            >
              <SelectTrigger className='w-full text-sm'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='body' className='text-sm'>
                  In Request Body
                </SelectItem>
                <SelectItem value='header' className='text-sm'>
                  As Basic Auth Header
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
        )}
      </div>

      {/* Scope / State group. */}
      <div>
        <div className={showState ? 'grid grid-cols-2 gap-3' : ''}>
          <div>
            <Label className='mb-1.5 block text-sm font-medium'>Scope</Label>
            <SingleLineEditor
              className='text-sm'
              placeholder='read write'
              value={o.scope}
              onChange={(newVal) => patchOAuth2({ scope: newVal })}
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
          {showState && (
            <div>
              <Label className='mb-1.5 block text-sm font-medium'>State</Label>
              <SingleLineEditor
                className='text-sm'
                placeholder='Auto-generated if empty'
                value={o.state}
                onChange={(newVal) => patchOAuth2({ state: newVal })}
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          )}
        </div>
      </div>

      {/* Behavior options — each checkbox is a full-width row for a larger target. */}
      {(showPkce || showSystemBrowser) && (
        <div className='space-y-2 pt-1'>
          {showPkce && (
            <label
              htmlFor='oauth2-use-pkce'
              className='flex items-center gap-3 cursor-pointer py-1 rounded group'
            >
              <Checkbox
                id='oauth2-use-pkce'
                checked={o.usePkce}
                onCheckedChange={(checked) => patchOAuth2({ usePkce: !!checked })}
              />
              <span className='text-sm text-foreground/80 group-hover:text-foreground transition-colors select-none'>
                Use PKCE (S256)
              </span>
            </label>
          )}
          {showSystemBrowser && (
            <label
              htmlFor='oauth2-use-system-browser'
              className='flex items-center gap-3 cursor-pointer py-1 rounded group'
            >
              <Checkbox
                id='oauth2-use-system-browser'
                checked={o.useSystemBrowser}
                onCheckedChange={(checked) => patchOAuth2({ useSystemBrowser: !!checked })}
              />
              <span className='text-sm text-foreground/80 group-hover:text-foreground transition-colors select-none'>
                Use system browser
              </span>
            </label>
          )}
        </div>
      )}
      <label
        htmlFor='oauth2-verify-ssl'
        className='flex items-center gap-3 cursor-pointer py-1 rounded group'
      >
        <Checkbox
          id='oauth2-verify-ssl'
          checked={o.verifySsl}
          onCheckedChange={(checked) => patchOAuth2({ verifySsl: !!checked })}
        />
        <span className='text-sm text-foreground/80 group-hover:text-foreground transition-colors select-none'>
          Verify SSL certificates
        </span>
      </label>
    </div>
  );
}
