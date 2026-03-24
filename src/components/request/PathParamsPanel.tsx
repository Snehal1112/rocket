import { useCallback } from 'react';
import { Check, X, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { KeyValueEntry } from '@/types/pane-types';

interface PathParamsPanelProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
}

export function PathParamsPanel({ params, onChange }: PathParamsPanelProps) {
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
    <>
      <div className="text-sm font-medium text-muted-foreground">Path Params</div>
      {params.map((entry) => (
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
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} path param ${entry.key || 'unnamed'}`}
          >
            {entry.enabled && <Check className="h-3 w-3" />}
          </Button>
          <Input
            placeholder="Path Key (e.g. customerId)"
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
            aria-label={`Remove path param ${entry.key || 'unnamed'}`}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      ))}
      <Button variant="ghost" size="sm" onClick={addEntry} className="text-xs">
        <Plus className="h-3 w-3 mr-1" />
        Add Path Param
      </Button>
    </>
  );
}
