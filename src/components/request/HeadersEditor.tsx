import { useCallback } from 'react';
import { Check, X, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { KeyValueEntry } from '@/types/pane-types';

interface HeadersEditorProps {
  headers: KeyValueEntry[];
  onChange: (headers: KeyValueEntry[]) => void;
}

export function HeadersEditor({ headers, onChange }: HeadersEditorProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(headers.map((h) => (h.id === id ? { ...h, ...patch } : h)));
    },
    [headers, onChange],
  );

  const removeEntry = useCallback(
    (id: string) => {
      onChange(headers.filter((h) => h.id !== id));
    },
    [headers, onChange],
  );

  const addEntry = useCallback(() => {
    onChange([
      ...headers,
      { id: crypto.randomUUID(), key: '', value: '', enabled: true },
    ]);
  }, [headers, onChange]);

  return (
    <div className="space-y-2">
      {headers.map((entry) => (
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
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} header ${entry.key || 'unnamed'}`}
          >
            {entry.enabled && <Check className="h-3 w-3" />}
          </Button>
          <Input
            placeholder="Key"
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Input
            placeholder="Value"
            value={entry.value}
            onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Button
            variant="ghost"
            size="icon"
            onClick={() => removeEntry(entry.id)}
            className="h-7 w-7"
            aria-label={`Remove header ${entry.key || 'unnamed'}`}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      ))}
      <Button variant="ghost" size="sm" onClick={addEntry} className="text-xs">
        <Plus className="h-3 w-3 mr-1" />
        Add Header
      </Button>
    </div>
  );
}
