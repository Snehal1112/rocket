import { useCallback } from 'react';
import { X, Plus } from 'lucide-react';
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
    <div className="space-y-1">
      <div className="grid grid-cols-[1fr_1fr_2.5rem_2rem] gap-1 px-1 text-xs font-medium text-muted-foreground">
        <span>Key</span>
        <span>Value</span>
        <span className="text-center">On</span>
        <span />
      </div>

      {headers.map((entry) => (
        <div
          key={entry.id}
          className="grid grid-cols-[1fr_1fr_2.5rem_2rem] items-center gap-1"
        >
          <Input
            className="h-7 text-xs"
            placeholder="Header name"
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
          />
          <Input
            className="h-7 text-xs"
            placeholder="Header value"
            value={entry.value}
            onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
          />
          <div className="flex justify-center">
            <input
              type="checkbox"
              className="h-3.5 w-3.5 accent-primary"
              checked={entry.enabled}
              onChange={(e) =>
                updateEntry(entry.id, { enabled: e.target.checked })
              }
              aria-label={`Enable header ${entry.key || 'unnamed'}`}
            />
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => removeEntry(entry.id)}
            aria-label={`Remove header ${entry.key || 'unnamed'}`}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}

      <Button
        variant="ghost"
        size="sm"
        className="mt-1 h-7 text-xs text-muted-foreground"
        onClick={addEntry}
      >
        <Plus className="mr-1 h-3.5 w-3.5" />
        Add header
      </Button>
    </div>
  );
}
