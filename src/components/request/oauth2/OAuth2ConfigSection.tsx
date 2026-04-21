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

  return (
    <div className='space-y-3'>
      {showAuthUrl && (
        <div>
          <Label className='mb-1 block'>Authorization URL</Label>
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
          <Label className='mb-1 block'>Access Token URL</Label>
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
          <Label className='mb-1 block'>Callback URL</Label>
          <div className='flex gap-1.5'>
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
              className='px-2 text-sm shrink-0'
              onClick={() => navigator.clipboard.writeText(o.callbackUrl)}
              title='Copy'
            >
              Copy
            </Button>
          </div>
        </div>
      )}

      {showSystemBrowser && (
        <div className='flex items-center gap-2'>
          <Checkbox
            id='oauth2-use-system-browser'
            checked={o.useSystemBrowser}
            onCheckedChange={(checked) => patchOAuth2({ useSystemBrowser: !!checked })}
          />
          <Label
            htmlFor='oauth2-use-system-browser'
            className='text-xs text-muted-foreground cursor-pointer'
          >
            Use system browser for authorization
          </Label>
        </div>
      )}

      <div className={showSecret ? 'grid grid-cols-2 gap-2' : ''}>
        <div>
          <Label className='mb-1 block'>Client ID</Label>
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
            <Label className='mb-1 block'>Client Secret</Label>
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

      <div>
        <Label className='mb-1 block'>Scope</Label>
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
          <Label className='mb-1 block'>State</Label>
          <SingleLineEditor
            className='text-sm'
            placeholder='Leave empty for auto-generated'
            value={o.state}
            onChange={(newVal) => patchOAuth2({ state: newVal })}
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
        </div>
      )}

      {showResourceOwner && (
        <div className='grid grid-cols-2 gap-2'>
          <div>
            <Label className='mb-1 block'>Username</Label>
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
            <Label className='mb-1 block'>Password</Label>
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
          <Label className='mb-1 block'>Add Credentials to</Label>
          <Select
            value={o.clientAuthentication}
            onValueChange={(v) => patchOAuth2({ clientAuthentication: v as 'header' | 'body' })}
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
      )}

      {showPkce && (
        <div className='flex items-center gap-2'>
          <Checkbox
            id='oauth2-use-pkce'
            checked={o.usePkce}
            onCheckedChange={(checked) => patchOAuth2({ usePkce: !!checked })}
          />
          <Label htmlFor='oauth2-use-pkce' className='text-xs text-muted-foreground cursor-pointer'>
            Use PKCE (S256)
          </Label>
        </div>
      )}

      <div className='flex items-center gap-2'>
        <Checkbox
          id='oauth2-verify-ssl'
          checked={o.verifySsl}
          onCheckedChange={(checked) => patchOAuth2({ verifySsl: !!checked })}
        />
        <Label htmlFor='oauth2-verify-ssl' className='text-xs text-muted-foreground cursor-pointer'>
          Verify SSL certificates
        </Label>
      </div>
    </div>
  );
}
