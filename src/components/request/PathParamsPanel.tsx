import { Link2 } from 'lucide-react';
import { useCallback } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Card, CardContent } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';

interface PathParamsPanelProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
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
    <Card>
      <CardContent className='p-4 space-y-3'>
        <div className='flex items-center gap-2 mb-1'>
          <Link2 className='h-3.5 w-3.5 text-muted-foreground' />
          <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
            Path
          </span>
          <span className='ml-auto text-[10px] text-muted-foreground/50'>derived from URL</span>
        </div>
        {params.length === 0 ? (
          <p className='text-xs text-muted-foreground italic'>
            No path params. Add <span className='font-mono'>:param</span> segments to the URL.
          </p>
        ) : (
          <>
            <div className='flex gap-2 items-center text-[10px] text-muted-foreground uppercase tracking-wider'>
              <div className='w-4' />
              <div className='flex-1'>Param</div>
              <div className='flex-1'>Value</div>
              <div className='w-7' />
            </div>
            {params.map((entry) => (
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
                  className='flex-1 text-xs bg-muted/50 cursor-default font-mono'
                />
                <SingleLineEditor
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
            ))}
          </>
        )}
      </CardContent>
    </Card>
  );
}
