import { useCallback } from 'react';
import { Trash2, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
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
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="h-7 text-xs">Key</TableHead>
            <TableHead className="h-7 text-xs">Value</TableHead>
            <TableHead className="h-7 w-10 text-center text-xs">On</TableHead>
            <TableHead className="h-7 w-8" />
          </TableRow>
        </TableHeader>
        <TableBody>
          {params.map((entry) => (
            <TableRow key={entry.id}>
              <TableCell className="p-1">
                <Input
                  className="h-7 text-xs"
                  placeholder="key"
                  value={entry.key}
                  onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
                />
              </TableCell>
              <TableCell className="p-1">
                <Input
                  className="h-7 text-xs"
                  placeholder="value"
                  value={entry.value}
                  onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
                />
              </TableCell>
              <TableCell className="p-1 text-center">
                <Checkbox
                  checked={entry.enabled}
                  onCheckedChange={(checked) =>
                    updateEntry(entry.id, { enabled: checked === true })
                  }
                  aria-label={`Enable parameter ${entry.key || 'unnamed'}`}
                  className="h-3.5 w-3.5"
                />
              </TableCell>
              <TableCell className="p-1">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => removeEntry(entry.id)}
                  aria-label={`Remove parameter ${entry.key || 'unnamed'}`}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>

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
