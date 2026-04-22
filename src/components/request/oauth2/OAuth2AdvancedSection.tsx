import { SingleLineEditor } from '@/components/editor';
import { Label } from '@/components/ui/label';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface OAuth2AdvancedSectionProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function OAuth2AdvancedSection({
  oauth2: o,
  patchOAuth2,
  variableContext,
  onNavigateToSource,
}: OAuth2AdvancedSectionProps) {
  // Implicit grant has no token endpoint, so no refresh URL is applicable.
  if (o.grantType === 'implicit') return null;

  return (
    <div>
      <Label className='mb-1.5 block text-sm font-medium'>Refresh Token URL</Label>
      <SingleLineEditor
        className='text-sm font-mono'
        placeholder='Leave empty to use the Token URL'
        value={o.refreshTokenUrl}
        onChange={(newVal) => patchOAuth2({ refreshTokenUrl: newVal })}
        variableContext={variableContext}
        onNavigateToSource={onNavigateToSource}
      />
    </div>
  );
}
