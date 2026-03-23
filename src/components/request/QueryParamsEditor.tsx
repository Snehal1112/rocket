import { useCallback } from 'react';
import { X, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { KeyValueEntry } from '@/types/pane-types';

interface QueryParamsEditorProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
}

export function QueryParamsEditor({ params, onChange }: QueryParamsEditorProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(params.map((p) => (p.id === id ? { ...p, ...patch } : p)));
    },
    [params, onChange],
  );

  const removeEntry = useCallback(
    (id: string) => {
      onChange(params.filter((p) => p.id !== id));
    },
    [params, onChange],
  );

  const addEntry = useCallback(() => {
    onChange([
      ...params,
      { id: crypto.randomUUID(), key: '', value: '', enabled: true },
    ]);
  }, [params, onChange]);

  return (
    <div className="space-y-1">
      {/* Header row. */}
      <div className="grid grid-cols-[1fr_1fr_2.5rem_2rem] gap-1 px-1 text-xs font-medium text-muted-foreground">
        <span>Key</span>
        <span>Value</span>
        <span className="text-center">On</span>
        <span />
      </div>

      {/* Parameter rows. */}
      {params.map((entry) => (
        <div
          key={entry.id}
          className="grid grid-cols-[1fr_1fr_2.5rem_2rem] items-center gap-1"
        >
          <Input
            className="h-7 text-xs"
            placeholder="key"
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
          />
          <Input
            className="h-7 text-xs"
            placeholder="value"
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
              aria-label={`Enable parameter ${entry.key || 'unnamed'}`}
            />
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => removeEntry(entry.id)}
            aria-label={`Remove parameter ${entry.key || 'unnamed'}`}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}

      {/* Add button. */}
      <Button
        variant="ghost"
        size="sm"
        className="mt-1 h-7 text-xs text-muted-foreground"
        onClick={addEntry}
      >
        <Plus className="mr-1 h-3.5 w-3.5" />
        Add parameter
      </Button>
    </div>
  );
}
