// src/components/environments/ExternalSecretsTab.tsx

import { Check, Download, Loader2, Plus, Save, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import type {
  ExternalSecretBinding,
  ExternalSecretRef,
  SecretManagerConnection,
} from '@/lib/tauri-api';
import { fetchExternalSecretNames } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

const GRID_COLS = 'grid-cols-[1fr_1fr_1fr_auto_28px]';

interface ExternalSecretsTabProps {
  bindings: ExternalSecretBinding[];
  onChange: (idx: number, patch: Partial<ExternalSecretBinding>) => void;
  onAdd: () => void;
  onRemove: (idx: number) => void;
  onSave: () => void;
  isDirty: boolean;
  saveState: SaveButtonState;
}

export function ExternalSecretsTab({
  bindings,
  onChange,
  onAdd,
  onRemove,
  onSave,
  isDirty,
  saveState,
}: ExternalSecretsTabProps) {
  const { data: connections = [] } = useSecretManagerConnections();

  return (
    <div className='flex-1 flex flex-col min-w-0'>
      <div
        className={cn(
          'grid min-w-0 items-center gap-1.5 px-3 pt-3 pb-1.5 border-b border-border/40 shrink-0',
          GRID_COLS,
        )}
      >
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Alias
        </p>
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Connection
        </p>
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Vault Name
        </p>
        <div />
        <div />
      </div>

      {bindings.length === 0 ? (
        <div className='flex-1 flex flex-col items-center justify-center gap-3 text-center px-6'>
          <p className='text-sm font-medium text-foreground'>No external secrets bound</p>
          <p className='text-xs text-muted-foreground leading-relaxed max-w-[260px]'>
            Bind a RocketVault connection and vault to fetch secret names for this environment.
          </p>
          <Button variant='outline' size='sm' onClick={onAdd} className='gap-1.5'>
            <Plus className='h-3.5 w-3.5' />
            Add Binding
          </Button>
        </div>
      ) : (
        <ScrollArea className='flex-1'>
          <div className='px-3 pt-2 pb-1 space-y-2'>
            {bindings.map((binding, idx) => (
              <BindingRow
                // biome-ignore lint/suspicious/noArrayIndexKey: index is stable here — rows are not reordered
                key={idx}
                idx={idx}
                binding={binding}
                connections={connections}
                onChange={onChange}
                onRemove={onRemove}
              />
            ))}
          </div>
        </ScrollArea>
      )}

      <div className='px-3 py-2 border-t border-border/40 flex items-center justify-between shrink-0'>
        {bindings.length > 0 ? (
          <Button
            variant='ghost'
            size='sm'
            onClick={onAdd}
            className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
          >
            <Plus className='h-3.5 w-3.5' />
            Add Binding
          </Button>
        ) : (
          <div />
        )}
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

interface BindingRowProps {
  idx: number;
  binding: ExternalSecretBinding;
  connections: SecretManagerConnection[];
  onChange: (idx: number, patch: Partial<ExternalSecretBinding>) => void;
  onRemove: (idx: number) => void;
}

function BindingRow({ idx, binding, connections, onChange, onRemove }: BindingRowProps) {
  // Local display state, seeded from props and echoed to the parent via
  // onChange. This keeps keystrokes and freshly-fetched secret names visible
  // immediately, independent of unrelated re-renders (e.g. the connections
  // query settling) and of whether the parent chooses to round-trip the
  // patch back into `binding` synchronously.
  const [alias, setAlias] = useState(binding.alias);
  const [vaultName, setVaultName] = useState(binding.vaultName);
  const [secretNames, setSecretNames] = useState<ExternalSecretRef[]>(binding.secretNames);
  const [isFetching, setIsFetching] = useState(false);

  // Resync when the bound data changes for a reason other than this row's
  // own edits, e.g. switching to a different environment.
  useEffect(() => setAlias(binding.alias), [binding.alias]);
  useEffect(() => setVaultName(binding.vaultName), [binding.vaultName]);
  useEffect(() => setSecretNames(binding.secretNames), [binding.secretNames]);

  const canFetch = !!binding.connectionId && !!vaultName && !isFetching;

  const fetchSecrets = async () => {
    if (!binding.connectionId || !vaultName) return;
    setIsFetching(true);
    try {
      // Wholesale replace, per spec 4.4 — never merged with the prior list.
      const names = await fetchExternalSecretNames(binding.connectionId, vaultName);
      setSecretNames(names);
      onChange(idx, { secretNames: names });
    } catch (err) {
      console.error('[ExternalSecretsTab] fetch secrets failed:', err);
      toast.error('Failed to fetch secrets');
    } finally {
      setIsFetching(false);
    }
  };

  return (
    <div className='space-y-1.5 pb-2 border-b border-border/20 last:border-0'>
      <div className={cn('grid min-w-0 items-center gap-1.5 group', GRID_COLS)}>
        <Input
          placeholder='Alias'
          value={alias}
          onChange={(e) => {
            setAlias(e.target.value);
            onChange(idx, { alias: e.target.value });
          }}
          className='h-7 min-w-0 text-xs font-mono'
          aria-label={`Alias for binding ${idx + 1}`}
        />
        <Select
          value={binding.connectionId}
          onValueChange={(v) => onChange(idx, { connectionId: v })}
        >
          <SelectTrigger
            className='h-7 min-w-0 text-xs'
            aria-label={`Connection for binding ${idx + 1}`}
          >
            <SelectValue placeholder='Select connection' />
          </SelectTrigger>
          <SelectContent>
            {connections.map((conn) => (
              <SelectItem key={conn.id} value={conn.id} className='text-xs'>
                {conn.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Input
          placeholder='Vault name'
          value={vaultName}
          onChange={(e) => {
            setVaultName(e.target.value);
            onChange(idx, { vaultName: e.target.value });
          }}
          className='h-7 min-w-0 text-xs font-mono'
          aria-label={`Vault name for binding ${idx + 1}`}
        />
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-xs gap-1.5 shrink-0'
          disabled={!canFetch}
          onClick={() => void fetchSecrets()}
        >
          {isFetching ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
          ) : (
            <Download className='h-3.5 w-3.5' />
          )}
          Fetch Secrets
        </Button>
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='h-6 w-6 shrink-0 opacity-60 group-hover:opacity-100 focus-visible:opacity-100 transition-opacity'
                onClick={() => onRemove(idx)}
                aria-label={`Delete binding ${idx + 1}`}
              >
                <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Delete binding</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>
      <div className='pl-0.5 flex flex-wrap gap-1'>
        {secretNames.length === 0 ? (
          <p className='text-[11px] text-muted-foreground/70'>No secrets fetched yet.</p>
        ) : (
          secretNames.map((ref) => (
            <Badge key={ref.secretId} variant='secondary' className='text-[11px] font-mono'>
              {ref.name}
            </Badge>
          ))
        )}
      </div>
    </div>
  );
}
