import { Plus, X } from 'lucide-react';
import { useCallback, useMemo } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { AuthState, OAuth2AdditionalParam } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;
type ParamKey = 'authParams' | 'tokenParams' | 'refreshParams';

interface OAuth2AdditionalParamsProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

interface ParamListProps {
  params: OAuth2AdditionalParam[];
  onChange: (params: OAuth2AdditionalParam[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

function ParamList({ params, onChange, variableContext, onNavigateToSource }: ParamListProps) {
  const updateAt = useCallback(
    (idx: number, patch: Partial<OAuth2AdditionalParam>) => {
      onChange(params.map((p, i) => (i === idx ? { ...p, ...patch } : p)));
    },
    [params, onChange],
  );

  const removeAt = useCallback(
    (idx: number) => {
      onChange(params.filter((_, i) => i !== idx));
    },
    [params, onChange],
  );

  const addRow = useCallback(() => {
    onChange([...params, { key: '', value: '', sendIn: 'body', enabled: true }]);
  }, [params, onChange]);

  return (
    <div className='space-y-2'>
      {params.length > 0 && (
        <div className='flex gap-2 items-center text-[10px] text-muted-foreground uppercase tracking-wider'>
          <div className='w-4' />
          <div className='flex-1'>Key</div>
          <div className='flex-1'>Value</div>
          <div className='w-28'>Send In</div>
          <div className='w-7' />
        </div>
      )}
      {params.map((p, idx) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: array index is the stable identity for row state here
        <div key={idx} className='flex gap-2 items-center'>
          <Checkbox
            checked={p.enabled}
            onCheckedChange={(checked) => updateAt(idx, { enabled: !!checked })}
            aria-label={`${p.enabled ? 'Disable' : 'Enable'} ${p.key || 'unnamed'}`}
          />
          <Input
            aria-label={`Key for row ${idx + 1}`}
            placeholder='Key'
            value={p.key}
            onChange={(e) => updateAt(idx, { key: e.target.value })}
            className='flex-1 text-xs font-mono'
          />
          {/* biome-ignore lint/a11y/useSemanticElements: fieldset breaks flex layout; div role=group labels the CodeMirror editor which has no aria-label prop */}
          <div role='group' aria-label={`Value for row ${idx + 1}`} className='flex-1'>
            <SingleLineEditor
              placeholder='Value'
              value={p.value}
              onChange={(newVal) => updateAt(idx, { value: newVal })}
              className='text-xs'
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
          <Select
            value={p.sendIn}
            onValueChange={(v) => updateAt(idx, { sendIn: v as 'queryparams' | 'body' })}
          >
            <SelectTrigger className='w-28 text-xs'>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value='queryparams' className='text-xs'>
                Query Params
              </SelectItem>
              <SelectItem value='body' className='text-xs'>
                Body
              </SelectItem>
            </SelectContent>
          </Select>
          <Button
            variant='ghost'
            size='icon'
            onClick={() => removeAt(idx)}
            className='h-7 w-7'
            aria-label={`Remove ${p.key || 'unnamed'}`}
          >
            <X className='h-3.5 w-3.5' />
          </Button>
        </div>
      ))}
      <Button variant='ghost' size='sm' onClick={addRow} className='text-xs'>
        <Plus className='h-3.5 w-3.5 mr-1' />
        Add Parameter
      </Button>
    </div>
  );
}

export function OAuth2AdditionalParams({
  oauth2: o,
  patchOAuth2,
  variableContext,
  onNavigateToSource,
}: OAuth2AdditionalParamsProps) {
  // Tab visibility per grant type. See spec Section 4.
  const tabs = useMemo(() => {
    const list: { key: ParamKey; label: string }[] = [];
    if (o.grantType === 'authorization_code' || o.grantType === 'implicit') {
      list.push({ key: 'authParams', label: 'Authorization' });
    }
    if (o.grantType !== 'implicit') {
      list.push({ key: 'tokenParams', label: 'Token' });
    }
    list.push({ key: 'refreshParams', label: 'Refresh' });
    return list;
  }, [o.grantType]);

  if (tabs.length === 0) return null;

  const patchList = (key: ParamKey) => (next: OAuth2AdditionalParam[]) =>
    patchOAuth2({ [key]: next } as Partial<OAuth2State>);

  return (
    <div className='space-y-2'>
      <Tabs defaultValue={tabs[0].key} className='w-full'>
        <TabsList>
          {tabs.map((t) => (
            <TabsTrigger key={t.key} value={t.key} className='text-xs'>
              {t.label}
            </TabsTrigger>
          ))}
        </TabsList>
        {tabs.map((t) => (
          <TabsContent key={t.key} value={t.key} className='mt-3'>
            <ParamList
              params={o[t.key]}
              onChange={patchList(t.key)}
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </TabsContent>
        ))}
      </Tabs>
    </div>
  );
}
