import { Braces, Eye, EyeOff, Plus, X } from 'lucide-react';
import { useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { CollectionVariable } from '@/lib/tauri-api';

interface CollectionVariablesEditorProps {
  variables: CollectionVariable[];
  onChange: (variables: CollectionVariable[]) => void;
  /** When false, hides the built-in description banner. Default: true. */
  showDescription?: boolean;
}

export function CollectionVariablesEditor({
  variables,
  onChange,
  showDescription = true,
}: CollectionVariablesEditorProps) {
  const updateVar = useCallback(
    (index: number, patch: Partial<CollectionVariable>) => {
      const updated = variables.map((v, i) => (i === index ? { ...v, ...patch } : v));
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
    <div className='flex flex-col'>
      {showDescription && (
        <div className='rounded-md border border-border bg-muted/30 px-3 py-2 mb-3'>
          <p className='text-xs text-muted-foreground'>
            Collection variables are available to all requests via {'{{variable}}'} syntax.
            Environment variables take precedence when both define the same key.
          </p>
        </div>
      )}

      {variables.length > 0 && (
        <>
          {/* Header row. */}
          <div className='flex items-center gap-1 border-b bg-muted/40 px-2 py-1.5 text-xs font-medium text-muted-foreground'>
            {/* Checkbox spacer. */}
            <span className='w-6 shrink-0' />
            <span className='flex-1 min-w-0'>Variable</span>
            <span className='flex-1 min-w-0'>Initial Value</span>
            <span className='flex-1 min-w-0'>Current Value</span>
            {/* Secret + remove spacer. */}
            <span className='w-14 shrink-0' />
          </div>

          {/* Data rows. */}
          {variables.map((v, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: index is stable — rows are not reordered
            <div
              key={i}
              className='flex items-center gap-1 border-b px-2 py-1 hover:bg-muted/20'
            >
              {/* Enabled toggle. */}
              <div className='w-6 shrink-0 flex items-center'>
                <Checkbox
                  checked={v.enabled}
                  onCheckedChange={(checked) => updateVar(i, { enabled: !!checked })}
                  aria-label={v.enabled ? 'Disable variable' : 'Enable variable'}
                />
              </div>

              {/* Variable name. */}
              <div className='flex-1 min-w-0'>
                <Input
                  aria-label={`Variable name, row ${i + 1}`}
                  placeholder='VARIABLE_NAME'
                  value={v.key}
                  onChange={(e) => updateVar(i, { key: e.target.value })}
                  className='h-7 text-xs font-mono'
                />
              </div>

              {/* Initial value. */}
              <div className='flex-1 min-w-0'>
                <Input
                  aria-label={`Initial value, row ${i + 1}`}
                  placeholder='Default value'
                  value={v.initialValue}
                  onChange={(e) => updateVar(i, { initialValue: e.target.value })}
                  className='h-7 text-xs font-mono'
                  type={v.secret ? 'password' : 'text'}
                />
              </div>

              {/* Current value. */}
              <div className='flex-1 min-w-0'>
                <Input
                  aria-label={`Current value, row ${i + 1}`}
                  placeholder='Current value'
                  value={v.value}
                  onChange={(e) => updateVar(i, { value: e.target.value })}
                  className='h-7 text-xs font-mono'
                  type={v.secret ? 'password' : 'text'}
                />
              </div>

              {/* Secret toggle + remove. */}
              <div className='w-14 shrink-0 flex items-center justify-end gap-0.5'>
                <Button
                  variant='ghost'
                  size='icon'
                  className='h-6 w-6'
                  onClick={() => updateVar(i, { secret: !v.secret })}
                  title={v.secret ? 'Show value' : 'Hide value (mark as secret)'}
                >
                  {v.secret ? (
                    <EyeOff className='h-3.5 w-3.5 text-muted-foreground' />
                  ) : (
                    <Eye className='h-3.5 w-3.5 text-muted-foreground' />
                  )}
                </Button>
                <Button
                  variant='ghost'
                  size='icon'
                  className='h-6 w-6'
                  onClick={() => removeVar(i)}
                  aria-label={`Delete variable ${i + 1}`}
                >
                  <X className='h-3.5 w-3.5' />
                </Button>
              </div>
            </div>
          ))}
        </>
      )}

      {variables.length === 0 ? (
        <div className='flex flex-col items-center justify-center gap-4 py-10 text-center'>
          <div className='h-12 w-12 rounded-xl bg-muted/40 border border-border/50 flex items-center justify-center'>
            <Braces className='h-5 w-5 text-muted-foreground/30' />
          </div>
          <div className='space-y-1.5'>
            <p className='text-xs font-medium text-muted-foreground/70'>No variables yet</p>
            <p className='text-[11px] text-muted-foreground/40 max-w-52 leading-relaxed'>
              Variables let you reuse values across requests using {'{{variable}}'} syntax.
            </p>
          </div>
          <Button size='sm' variant='outline' onClick={addVar}>
            <Plus className='mr-1.5 h-3.5 w-3.5' />
            Add Variable
          </Button>
        </div>
      ) : (
        <div className='px-2 py-2'>
          <Button variant='ghost' size='sm' onClick={addVar} className='text-sm'>
            <Plus className='h-3.5 w-3.5 mr-1' />
            Add Variable
          </Button>
        </div>
      )}
    </div>
  );
}
