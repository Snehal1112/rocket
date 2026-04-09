import { useCallback } from 'react';
import { VariableAwareInput } from '@/components/request/VariableAwareInput';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';

interface PathParamsPanelProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}

/**
 * Read-only path params display. Keys are derived from the URL
 * (e.g. `:id` segments) and cannot be added, removed, or renamed.
 * Only values and the enabled toggle are editable.
 */
export function PathParamsPanel({
  params,
  onChange,
  variableContext,
  onNavigateToSource,
}: PathParamsPanelProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(params.map((e) => (e.id === id ? { ...e, ...patch } : e)));
    },
    [params, onChange],
  );

  return (
    <div className='space-y-2'>
      <div className='text-xs font-medium text-muted-foreground'>Path</div>
      {params.length === 0 ? (
        <p className='text-xs text-muted-foreground italic px-1'>
          No path params. Add <span className='font-mono'>:param</span> segments to the URL.
        </p>
      ) : (
        params.map((entry) => (
          <div key={entry.id} className='flex gap-2 items-center'>
            <Checkbox
              checked={entry.enabled}
              onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
              aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key}`}
            />
            <Input
              value={entry.key}
              readOnly
              tabIndex={-1}
              className='flex-1 text-xs bg-muted/50 cursor-default'
            />
            <VariableAwareInput
              placeholder='Value'
              value={entry.value}
              onChange={(newVal) => updateEntry(entry.id, { value: newVal })}
              className='flex-1 text-xs'
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
            {/* No remove button — path params are controlled by the URL. */}
            <div className='w-7' />
          </div>
        ))
      )}
    </div>
  );
}
