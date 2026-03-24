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
          {headers.map((entry) => (
            <TableRow key={entry.id}>
              <TableCell className="p-1">
                <Input
                  className="h-7 text-xs"
                  placeholder="Header name"
                  value={entry.key}
                  onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
                />
              </TableCell>
              <TableCell className="p-1">
                <Input
                  className="h-7 text-xs"
                  placeholder="Header value"
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
                  aria-label={`Enable header ${entry.key || 'unnamed'}`}
                  className="h-3.5 w-3.5"
                />
              </TableCell>
              <TableCell className="p-1">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => removeEntry(entry.id)}
                  aria-label={`Remove header ${entry.key || 'unnamed'}`}
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
        Add header
      </Button>
    </div>
  );
}
