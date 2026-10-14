import { Plus, X } from 'lucide-react';
import { useId } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

/** `key` is a stable React key and is never edited. */
export interface KeyValueRow {
  key: string;
  name: string;
  value: string;
}

export interface KeyValueRowsProps {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  columns: 'pair' | 'single';
  nameLabel?: string;
  valueLabel: string;
  addLabel: string;
  rowErrors?: (string | null)[];
}

/**
 * Editable rows. Pair mode edits name and value, single mode edits only the value.
 * It never validates: a row shows an error only when `rowErrors` gives one for its index.
 * Accessible names: inputs "<label> <n>", remove "Remove <label lowercase> <n>", add = addLabel.
 */
export function KeyValueRows({
  rows,
  onChange,
  columns,
  nameLabel = 'Name',
  valueLabel,
  addLabel,
  rowErrors,
}: KeyValueRowsProps) {
  const baseId = useId();
  const isPair = columns === 'pair';
  const gridClass = isPair ? 'grid-cols-[1fr_1fr_32px]' : 'grid-cols-[1fr_32px]';
  const removeNoun = (isPair ? nameLabel : valueLabel).toLowerCase();

  const update = (key: string, patch: Partial<KeyValueRow>) =>
    onChange(rows.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  const remove = (key: string) => onChange(rows.filter((row) => row.key !== key));

  const add = () => onChange([...rows, { key: crypto.randomUUID(), name: '', value: '' }]);

  return (
    <TooltipProvider>
      <div className='space-y-2'>
        {rows.length > 0 && (
          <div
            data-slot='kv-header'
            className={cn('grid gap-2 text-xs text-muted-foreground', gridClass)}
          >
            {isPair && <span>{nameLabel}</span>}
            <span>{valueLabel}</span>
            <span aria-hidden='true' />
          </div>
        )}
        {rows.map((row, index) => {
          const error = rowErrors?.[index] ?? null;
          const errorId = `${baseId}-${row.key}-error`;
          const describedBy = error ? errorId : undefined;
          return (
            <div key={row.key} className='space-y-1'>
              <div data-slot='kv-row' className={cn('grid items-center gap-2', gridClass)}>
                {isPair && (
                  <Input
                    id={`${baseId}-${row.key}-name`}
                    aria-label={`${nameLabel} ${index + 1}`}
                    aria-invalid={error !== null}
                    aria-describedby={describedBy}
                    value={row.name}
                    onChange={(e) => update(row.key, { name: e.target.value })}
                    className='h-8 text-sm'
                  />
                )}
                <Input
                  id={`${baseId}-${row.key}-value`}
                  aria-label={`${valueLabel} ${index + 1}`}
                  aria-invalid={error !== null}
                  aria-describedby={describedBy}
                  value={row.value}
                  onChange={(e) => update(row.key, { value: e.target.value })}
                  className='h-8 text-sm'
                />
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant='ghost'
                      size='icon'
                      className='size-8'
                      aria-label={`Remove ${removeNoun} ${index + 1}`}
                      onClick={() => remove(row.key)}
                    >
                      <X className='h-3.5 w-3.5' aria-hidden='true' />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Remove</TooltipContent>
                </Tooltip>
              </div>
              {error && (
                <p id={errorId} role='alert' className='break-words text-xs text-destructive'>
                  {error}
                </p>
              )}
            </div>
          );
        })}
        <div className='flex justify-start'>
          <Button variant='outline' size='sm' onClick={add}>
            <Plus className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
            {addLabel}
          </Button>
        </div>
      </div>
    </TooltipProvider>
  );
}
