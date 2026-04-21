import { SingleLineEditor } from '@/components/editor';
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

interface OAuth2TokenSectionProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function OAuth2TokenSection({
  oauth2: o,
  patchOAuth2,
  variableContext,
  onNavigateToSource,
}: OAuth2TokenSectionProps) {
  return (
    <div className='space-y-3'>
      <div>
        <Label className='mb-1 block'>Token Source</Label>
        <Select
          value={o.tokenSource}
          onValueChange={(v) => patchOAuth2({ tokenSource: v as 'accessToken' | 'idToken' })}
        >
          <SelectTrigger className='w-full text-sm'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='accessToken' className='text-sm'>
              Access Token
            </SelectItem>
            <SelectItem value='idToken' className='text-sm'>
              ID Token
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div>
        <Label className='mb-1 block'>Token ID</Label>
        <SingleLineEditor
          className='text-sm'
          placeholder='(optional)'
          value={o.tokenId}
          onChange={(newVal) => patchOAuth2({ tokenId: newVal })}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      </div>

      <div>
        <Label className='mb-1 block'>Add Token To</Label>
        <Select
          value={o.addTokenTo}
          onValueChange={(v) => patchOAuth2({ addTokenTo: v as 'header' | 'queryParams' })}
        >
          <SelectTrigger className='w-full text-sm'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='header' className='text-sm'>
              Headers
            </SelectItem>
            <SelectItem value='queryParams' className='text-sm'>
              Query Params
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      {o.addTokenTo === 'header' && (
        <div>
          <Label className='mb-1 block'>Header Prefix</Label>
          <SingleLineEditor
            className='text-sm'
            value={o.headerPrefix}
            onChange={(newVal) => patchOAuth2({ headerPrefix: newVal })}
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
        </div>
      )}
    </div>
  );
}
