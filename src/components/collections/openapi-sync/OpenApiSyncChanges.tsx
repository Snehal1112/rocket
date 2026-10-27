import { useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { StateBlock } from '@/components/ui/state-block';
import { useOpenApiSyncChanges, useResetOpenApiSync } from '@/lib/queries/openapi-sync-queries';
import type { OpenApiChangeRow } from '@/lib/tauri-api';
import { fieldLabel, resetAllConfirmText, resetConfirmText } from './openApiSyncView';
import { ResetConfirmDialog } from './ResetConfirmDialog';
import { syncErrorMessage } from './syncErrors';

type Kind = 'modified' | 'added' | 'deleted';

const KIND_LABEL: Record<Kind, string> = {
  modified: 'Modified',
  added: 'Added',
  deleted: 'Deleted',
};

const GROUP_TITLE: Record<Kind, string> = {
  modified: 'Modified',
  added: 'Added by you',
  deleted: 'Deleted by you',
};

interface ResetControl {
  enabled: boolean;
  onReset: (row: OpenApiChangeRow) => void;
}

function ChangeRow({
  row,
  kind,
  showFields,
  reset,
}: {
  row: OpenApiChangeRow;
  kind: Kind;
  showFields: boolean;
  reset?: ResetControl;
}) {
  // Deleted rows have no method or URL, so the path or the name stands in.
  const target = row.url || row.requestPath || row.name;
  return (
    <li className='flex flex-col gap-1 border-b border-border/60 px-3 py-2 text-sm last:border-b-0'>
      <div className='flex min-w-0 items-center gap-2'>
        <Badge variant='outline'>{KIND_LABEL[kind]}</Badge>
        {row.method && (
          <span className='shrink-0 font-mono text-xs font-semibold'>{row.method}</span>
        )}
        <span className='min-w-0 flex-1 break-all font-mono text-xs'>{target}</span>
        {kind === 'modified' && reset && row.key && (
          <Button
            variant='outline'
            size='sm'
            className='h-7 shrink-0'
            disabled={!reset.enabled}
            onClick={() => reset.onReset(row)}
          >
            Reset to spec
          </Button>
        )}
      </div>
      {showFields && kind === 'modified' && row.fields.length > 0 && (
        <div className='flex flex-wrap gap-1'>
          {row.fields.map((f) => (
            <Badge key={f} variant='secondary'>
              {fieldLabel(f)}
            </Badge>
          ))}
        </div>
      )}
    </li>
  );
}

function Group({
  kind,
  rows,
  showFields,
  reset,
  onResetAll,
}: {
  kind: Kind;
  rows: OpenApiChangeRow[];
  showFields: boolean;
  reset?: ResetControl;
  onResetAll?: () => void;
}) {
  if (rows.length === 0) return null;
  return (
    <section className='flex flex-col gap-1'>
      <div className='flex items-center justify-between gap-2'>
        <h3 className='text-sm font-medium'>{`${GROUP_TITLE[kind]} (${rows.length})`}</h3>
        {onResetAll && reset && (
          <Button variant='outline' size='sm' disabled={!reset.enabled} onClick={onResetAll}>
            {`Reset all to spec (${rows.length})`}
          </Button>
        )}
      </div>
      <ul className='rounded-md border border-border/70'>
        {rows.map((r) => (
          <ChangeRow key={r.uid} row={r} kind={kind} showFields={showFields} reset={reset} />
        ))}
      </ul>
    </section>
  );
}

interface PendingReset {
  keys: string[];
  body: string;
}

/** What the user changed against the last synced spec, with a way to reset it to the spec. */
export function OpenApiSyncChanges({ collection }: { collection: string }) {
  const changes = useOpenApiSyncChanges(collection);
  const reset = useResetOpenApiSync(collection);
  const [pending, setPending] = useState<PendingReset | null>(null);
  if (changes.isError) {
    return <StateBlock tone='error'>{syncErrorMessage(changes.error)}</StateBlock>;
  }
  const c = changes.data;
  if (!c) return <StateBlock tone='loading'>Loading your changes...</StateBlock>;

  const total = c.modified.length + c.added.length + c.deleted.length;
  if (total === 0) return <StateBlock tone='empty'>You have not changed anything.</StateBlock>;
  const control: ResetControl = {
    enabled: c.resetAvailable && !reset.isPending,
    onReset: (row) => {
      if (!row.key) return;
      reset.reset();
      setPending({ keys: [row.key], body: resetConfirmText(row) });
    },
  };
  const resetAll = () => {
    const keys = c.modified.flatMap((r) => (r.key ? [r.key] : []));
    reset.reset();
    setPending({ keys, body: resetAllConfirmText(keys.length) });
  };
  return (
    <div className='flex flex-col gap-4'>
      <p className='text-xs text-muted-foreground'>
        Differences between your collection and the last synced spec. Sync never overwrites these
        without asking.
      </p>
      {!c.baselineTrusted && (
        <p className='text-xs text-muted-foreground'>
          These differences are approximate because the sync data is from an older version.
        </p>
      )}
      {!c.resetAvailable && c.modified.length > 0 && (
        <p className='text-xs text-muted-foreground'>
          These requests cannot be reset because the sync data is from an older version.
        </p>
      )}
      {reset.isError && <StateBlock tone='error'>{syncErrorMessage(reset.error)}</StateBlock>}
      <Group
        kind='modified'
        rows={c.modified}
        showFields={c.baselineTrusted}
        reset={control}
        onResetAll={resetAll}
      />
      <Group kind='added' rows={c.added} showFields={false} />
      <Group kind='deleted' rows={c.deleted} showFields={false} />
      <p className='text-xs text-muted-foreground'>
        Scripts, tests and assertions are not tracked here.
      </p>
      <ResetConfirmDialog
        open={pending !== null}
        onOpenChange={(open) => {
          if (!open) setPending(null);
        }}
        title='Reset to the spec?'
        body={pending?.body ?? ''}
        confirmLabel='Reset'
        onConfirm={() => {
          if (pending) reset.mutate(pending.keys);
          setPending(null);
        }}
      />
    </div>
  );
}
