import { useCallback } from 'react';
import { Check, X, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { KeyValueEntry } from '@/types/pane-types';

interface KeyValueEditorProps {
  entries: KeyValueEntry[];
  onChange: (entries: KeyValueEntry[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
  label?: string;
}

export function KeyValueEditor({
  entries,
  onChange,
  keyPlaceholder = 'Key',
  valuePlaceholder = 'Value',
  addLabel = 'Add Entry',
  label,
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
    onChange([
      ...entries,
      { id: crypto.randomUUID(), key: '', value: '', enabled: true },
    ]);
  }, [entries, onChange]);

  return (
    <div className="space-y-2">
      {label && <div className="text-sm font-medium text-muted-foreground">{label}</div>}
      {entries.map((entry) => (
        <div key={entry.id} className="flex gap-2 items-center">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => updateEntry(entry.id, { enabled: !entry.enabled })}
            className={`w-4 h-4 rounded border p-0 ${
              entry.enabled
                ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                : 'border-gray-300 hover:bg-muted'
            }`}
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
          >
            {entry.enabled && <Check className="h-3 w-3" />}
          </Button>
          <Input
            placeholder={keyPlaceholder}
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Input
            placeholder={valuePlaceholder}
            value={entry.value}
            onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Button
            variant="ghost"
            size="icon"
            onClick={() => removeEntry(entry.id)}
            className="h-7 w-7"
            aria-label={`Remove ${entry.key || 'unnamed'}`}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      ))}
      <Button variant="ghost" size="sm" onClick={addEntry} className="text-xs">
        <Plus className="h-3 w-3 mr-1" />
        {addLabel}
      </Button>
    </div>
  );
}
