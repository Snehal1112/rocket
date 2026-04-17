import { Plus, X } from 'lucide-react';
import { useCallback } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';

interface KeyValueEditorProps {
  entries: KeyValueEntry[];
  onChange: (entries: KeyValueEntry[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
  label?: string;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function KeyValueEditor({
  entries,
  onChange,
  keyPlaceholder = 'Key',
  valuePlaceholder = 'Value',
  addLabel = 'Add Entry',
  label,
  variableContext,
  onNavigateToSource,
}: KeyValueEditorProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(entries.map((e) => (e.id === id ? { ...e, ...patch } : e)));
    },
    [entries, onChange],
  );

  const removeEntry = useCallback(
    (id: string) => {
      onChange(entries.filter((e) => e.id !== id));
    },
    [entries, onChange],
  );

  const addEntry = useCallback(() => {
    onChange([...entries, { id: crypto.randomUUID(), key: '', value: '', enabled: true }]);
  }, [entries, onChange]);

  return (
    <div className='space-y-2'>
      {label && <div className='text-xs font-medium text-muted-foreground'>{label}</div>}
      {entries.length > 0 && (
        <div className='flex gap-2 items-center text-[10px] text-muted-foreground uppercase tracking-wider'>
          <div className='w-4' />
          <div className='flex-1'>{keyPlaceholder}</div>
          <div className='flex-1'>{valuePlaceholder}</div>
          <div className='w-7' />
        </div>
      )}
      {entries.map((entry) => (
        <div key={entry.id} className='flex gap-2 items-center'>
          <Checkbox
            checked={entry.enabled}
            onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
          />
          {/* Key column stays as plain Input — variable templates are for values only. */}
          <Input
            placeholder={keyPlaceholder}
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
            className='flex-1 text-xs'
          />
          <SingleLineEditor
            placeholder={valuePlaceholder}
            value={entry.value}
            onChange={(newVal) => updateEntry(entry.id, { value: newVal })}
            className='flex-1 text-xs'
            variableContext={variableContext}
            onNavigateToSource={onNavigateToSource}
          />
          <Button
            variant='ghost'
            size='icon'
            onClick={() => removeEntry(entry.id)}
            className='h-7 w-7'
            aria-label={`Remove ${entry.key || 'unnamed'}`}
          >
            <X className='h-3.5 w-3.5' />
          </Button>
        </div>
      ))}
      <Button variant='ghost' size='sm' onClick={addEntry} className='text-xs'>
        <Plus className='h-3.5 w-3.5 mr-1' />
        {addLabel}
      </Button>
    </div>
  );
}
