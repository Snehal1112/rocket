import { useCallback } from 'react';
import { Plus, X, Eye, EyeOff } from 'lucide-react';
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
import type { CollectionVariable } from '@/lib/tauri-api';

interface CollectionVariablesEditorProps {
  variables: CollectionVariable[];
  onChange: (variables: CollectionVariable[]) => void;
}

export function CollectionVariablesEditor({
  variables,
  onChange,
}: CollectionVariablesEditorProps) {
  const updateVar = useCallback(
    (index: number, patch: Partial<CollectionVariable>) => {
      const updated = variables.map((v, i) =>
        i === index ? { ...v, ...patch } : v,
      );
      onChange(updated);
    },
    [variables, onChange],
  );

  const removeVar = useCallback(
    (index: number) => {
      onChange(variables.filter((_, i) => i !== index));
    },
    [variables, onChange],
  );

  const addVar = useCallback(() => {
    onChange([
      ...variables,
      { key: '', value: '', initialValue: '', enabled: true, secret: false },
    ]);
  }, [variables, onChange]);

  return (
    <div className="space-y-3">
      <div className="rounded-md border border-border bg-muted/30 px-3 py-2">
        <p className="text-xs text-muted-foreground">
          Collection variables are available to all requests via {'{{variable}}'} syntax.
          Environment variables take precedence when both define the same key.
        </p>
      </div>

      {variables.length > 0 ? (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="h-8 w-8 text-xs" />
              <TableHead className="h-8 text-xs font-semibold">Variable</TableHead>
              <TableHead className="h-8 text-xs font-semibold">Initial Value</TableHead>
              <TableHead className="h-8 text-xs font-semibold">Current Value</TableHead>
              <TableHead className="h-8 w-10 text-xs" />
              <TableHead className="h-8 w-8 text-xs" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {variables.map((v, i) => (
              <TableRow key={i}>
                {/* Enabled toggle. */}
                <TableCell className="px-2 py-1.5">
                  <Checkbox
                    checked={v.enabled}
                    onCheckedChange={(checked) => updateVar(i, { enabled: !!checked })}
                    aria-label={v.enabled ? 'Disable variable' : 'Enable variable'}
                  />
                </TableCell>

                {/* Key. */}
                <TableCell className="px-1.5 py-1.5">
                  <Input
                    placeholder="VARIABLE_NAME"
                    value={v.key}
                    onChange={(e) => updateVar(i, { key: e.target.value })}
                    className="text-sm font-mono"
                  />
                </TableCell>

                {/* Initial value. */}
                <TableCell className="px-1.5 py-1.5">
                  <Input
                    placeholder="Default value"
                    value={v.initialValue}
                    onChange={(e) => updateVar(i, { initialValue: e.target.value })}
                    className="text-sm"
                    type={v.secret ? 'password' : 'text'}
                  />
                </TableCell>

                {/* Current value. */}
                <TableCell className="px-1.5 py-1.5">
                  <Input
                    placeholder="Current value"
                    value={v.value}
                    onChange={(e) => updateVar(i, { value: e.target.value })}
                    className="text-sm"
                    type={v.secret ? 'password' : 'text'}
                  />
                </TableCell>

                {/* Secret toggle. */}
                <TableCell className="px-1.5 py-1.5">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    onClick={() => updateVar(i, { secret: !v.secret })}
                    title={v.secret ? 'Show value' : 'Hide value (mark as secret)'}
                  >
                    {v.secret ? (
                      <EyeOff className="h-3.5 w-3.5 text-muted-foreground" />
                    ) : (
                      <Eye className="h-3.5 w-3.5 text-muted-foreground" />
                    )}
                  </Button>
                </TableCell>

                {/* Remove. */}
                <TableCell className="px-1.5 py-1.5">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    onClick={() => removeVar(i)}
                    aria-label="Remove variable"
                  >
                    <X className="h-3.5 w-3.5" />
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      ) : (
        <div className="flex items-center justify-center py-8 text-sm text-muted-foreground">
          No collection variables defined.
        </div>
      )}

      <Button variant="ghost" size="sm" onClick={addVar} className="text-sm">
        <Plus className="h-3.5 w-3.5 mr-1" />
        Add Variable
      </Button>
    </div>
  );
}
