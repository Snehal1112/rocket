// src/components/environments/VariableTable.tsx

import { Check, Eye, EyeOff, Loader2, Plus, Save, X } from 'lucide-react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { SaveButtonState } from '@/hooks/use-save-button';
import type { Variable } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';

const GRID_COLS = 'grid-cols-[20px_1fr_1fr_52px]';

interface VariableTableProps {
  variables: Variable[];
  onChange: (idx: number, patch: Partial<Variable>) => void;
  onAdd: () => void;
  onRemove: (idx: number) => void;
  onSave: () => void;
  isDirty: boolean;
  saveState: SaveButtonState;
  variableContext?: Map<string, VariableScopeEntry>;
}

export function VariableTable({
  variables,
  onChange,
  onAdd,
  onRemove,
  onSave,
  isDirty,
  saveState,
  variableContext,
}: VariableTableProps) {
  const keyCounts = new Map<string, number>();
  for (const v of variables) {
    if (v.key === '') continue;
    keyCounts.set(v.key, (keyCounts.get(v.key) ?? 0) + 1);
  }

  return (
    <div className='flex-1 flex flex-col min-w-0'>
      <div
        className={cn(
          'grid min-w-0 items-center gap-1.5 px-3 pt-3 pb-1.5 border-b border-border/40 shrink-0',
          GRID_COLS,
        )}
      >
        <div />
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Key
        </p>
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Value
        </p>
        <div />
      </div>

      {variables.length === 0 ? (
        <div className='flex-1 flex flex-col items-center justify-center gap-3 text-center px-6'>
          <p className='text-sm font-medium text-foreground'>No variables yet</p>
          <p className='text-xs text-muted-foreground leading-relaxed max-w-[220px]'>
            Add a key and value to start using this environment.
          </p>
          <Button variant='outline' size='sm' onClick={onAdd} className='gap-1.5'>
            <Plus className='h-3.5 w-3.5' />
            Add Variable
          </Button>
        </div>
      ) : (
        <ScrollArea className='flex-1'>
          <div className='px-3 pt-2 pb-1 space-y-1'>
            {variables.map((variable, idx) => {
              const isDuplicate = variable.key !== '' && (keyCounts.get(variable.key) ?? 0) > 1;
              return (
                // biome-ignore lint/suspicious/noArrayIndexKey: index is stable here — rows are not reordered
                <div key={idx} className='space-y-0.5'>
                  <div
                    className={cn(
                      'grid min-w-0 items-center gap-1.5 group',
                      GRID_COLS,
                      !variable.enabled && 'opacity-50',
                    )}
                  >
                    <Checkbox
                      checked={variable.enabled}
                      onCheckedChange={(checked) => onChange(idx, { enabled: !!checked })}
                      aria-label={`${variable.enabled ? 'Disable' : 'Enable'} variable`}
                      className='shrink-0'
                    />
                    <Input
                      placeholder='Key'
                      value={variable.key}
                      onChange={(e) => onChange(idx, { key: e.target.value })}
                      className={cn(
                        'h-7 min-w-0 text-xs font-mono',
                        isDuplicate &&
                          'border-amber-500 focus-visible:ring-amber-500/50 dark:border-amber-500',
                      )}
                      aria-label={`Variable key ${idx + 1}`}
                    />
                    {/* biome-ignore lint/a11y/useSemanticElements: fieldset breaks flex layout; div role=group labels the CodeMirror editor which has no aria-label prop */}
                    <div
                      role='group'
                      aria-label={`Value for variable ${idx + 1}`}
                      className='min-w-0'
                    >
                      <SingleLineEditor
                        placeholder='Value'
                        value={variable.value}
                        onChange={(value) => onChange(idx, { value })}
                        isSecret={variable.secret}
                        variableContext={variableContext}
                        className='h-7 text-xs font-mono'
                      />
                    </div>
                    <div className='flex items-center gap-0.5 justify-end'>
                      <TooltipProvider>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              variant='ghost'
                              size='icon'
                              className='h-6 w-6 shrink-0 opacity-60 group-hover:opacity-100 focus-visible:opacity-100 transition-opacity'
                              onClick={() => onChange(idx, { secret: !variable.secret })}
                              aria-label={variable.secret ? 'Show value' : 'Hide value'}
                            >
                              {variable.secret ? (
                                <EyeOff className='h-3.5 w-3.5 text-muted-foreground' />
                              ) : (
                                <Eye className='h-3.5 w-3.5 text-muted-foreground' />
                              )}
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>
                            {variable.secret ? 'Show value' : 'Hide value'}
                          </TooltipContent>
                        </Tooltip>
                      </TooltipProvider>
                      <TooltipProvider>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              variant='ghost'
                              size='icon'
                              className='h-6 w-6 shrink-0 opacity-60 group-hover:opacity-100 focus-visible:opacity-100 transition-opacity'
                              onClick={() => onRemove(idx)}
                              aria-label={`Delete variable ${idx + 1}`}
                            >
                              <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>Delete variable</TooltipContent>
                        </Tooltip>
                      </TooltipProvider>
                    </div>
                  </div>
                  {isDuplicate && (
                    <p className='pl-[26px] text-[11px] text-amber-600 dark:text-amber-400'>
                      Duplicate key — only the last one will be saved
                    </p>
                  )}
                </div>
              );
            })}
          </div>
        </ScrollArea>
      )}

      <div className='px-3 py-2 border-t border-border/40 flex items-center justify-between shrink-0'>
        <Button
          variant='ghost'
          size='sm'
          onClick={onAdd}
          className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
        >
          <Plus className='h-3.5 w-3.5' />
          Add Variable
        </Button>
        <Button
          size='sm'
          onClick={onSave}
          disabled={!isDirty || saveState !== 'idle'}
          className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
        >
          {saveState === 'saving' ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : saveState === 'success' ? (
            <Check className='h-3.5 w-3.5' />
          ) : (
            <Save className='h-3.5 w-3.5' />
          )}
          {saveState === 'success' ? 'Saved' : 'Save'}
        </Button>
      </div>
    </div>
  );
}
