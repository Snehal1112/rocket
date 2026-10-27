import { useState } from 'react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { StateBlock } from '@/components/ui/state-block';
import {
  useDiscardOpenApiBackup,
  useOpenApiSyncChanges,
  useResetOpenApiSync,
  useRestoreOpenApiSync,
} from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiBackupRow, OpenApiChangeRow } from '@/lib/tauri-api';
import { fieldLabel, resetAllConfirmText, resetConfirmText } from './openApiSyncView';
import { openSyncRequest } from './openSyncRequest';
import { ResetConfirmDialog } from './ResetConfirmDialog';
import { syncErrorMessage } from './syncErrors';
import { ValueBox } from './ValueBox';

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
  collection,
  row,
  kind,
  showFields,
  reset,
}: {
  collection: string;
  row: OpenApiChangeRow;
  kind: Kind;
  showFields: boolean;
  reset?: ResetControl;
}) {
  // Deleted rows have no method or URL, so the path or the name stands in.
  const target = row.url || row.requestPath || row.name;
  const [open, setOpen] = useState(false);
  const [missing, setMissing] = useState(false);
  const canOpen = kind !== 'deleted' && row.requestPath !== null;
  const openRequest = async () => {
    if (!row.requestPath) return;
    const result = await openSyncRequest({
      collection,
      uid: row.uid,
      requestPath: row.requestPath,
    });
    setMissing(result === 'missing');
  };
  const canExpand = showFields && kind === 'modified' && row.changes.length > 0;
  return (
    <li className='flex flex-col gap-1 border-b border-border/60 px-3 py-2 text-sm last:border-b-0'>
      <div className='flex min-w-0 items-center gap-2'>
        <Badge variant='outline'>{KIND_LABEL[kind]}</Badge>
        {row.method && (
          <span className='shrink-0 font-mono text-xs font-semibold'>{row.method}</span>
        )}
        <span className='min-w-0 flex-1 break-all font-mono text-xs'>{target}</span>
        {canOpen && (
          <Button
            variant='ghost'
            size='sm'
            className='h-7 shrink-0'
            onClick={() => void openRequest()}
          >
            Open request
          </Button>
        )}
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
      {missing && (
        <p role='alert' className='text-xs text-destructive'>
          The request file was not found.
        </p>
      )}
      {showFields && kind === 'modified' && row.fields.length > 0 && (
        <div className='flex flex-wrap gap-1'>
          {row.fields.map((f) => (
            <Badge key={f} variant='secondary'>
              {fieldLabel(f)}
            </Badge>
          ))}
        </div>
      )}
      {canExpand && (
        <div className='flex flex-col gap-2'>
          <Button
            variant='link'
            size='sm'
            className='h-auto self-start p-0 text-xs'
            aria-expanded={open}
            onClick={() => setOpen((v) => !v)}
          >
            {open ? 'Hide changes' : 'Show changes'}
          </Button>
          {open &&
            row.changes.map((c) => (
              <div key={c.field} className='flex flex-col gap-2 rounded-md border p-3'>
                <span className='break-all text-sm font-medium'>{fieldLabel(c.field)}</span>
                <ValueBox title='Baseline' value={c.baseline} />
                <ValueBox title='Yours' value={c.yours} />
              </div>
            ))}
        </div>
      )}
    </li>
  );
}

function Group({
  collection,
  kind,
  rows,
  showFields,
  reset,
  onResetAll,
}: {
  collection: string;
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
          <ChangeRow
            key={r.uid}
            collection={collection}
            row={r}
            kind={kind}
            showFields={showFields}
            reset={reset}
          />
        ))}
      </ul>
    </section>
  );
}

interface PendingReset {
  keys: string[];
  body: string;
}

function BackupGroup({
  rows,
  busy,
  onRestore,
  onDiscard,
}: {
  rows: OpenApiBackupRow[];
  busy: boolean;
  onRestore: (row: OpenApiBackupRow) => void;
  onDiscard: (row: OpenApiBackupRow) => void;
}) {
  if (rows.length === 0) return null;
  return (
    <section className='flex flex-col gap-1'>
      <h3 className='text-sm font-medium'>{`Reset by you (${rows.length})`}</h3>
      <ul className='rounded-md border border-border/70'>
        {rows.map((r) => (
          <li
            key={r.key}
            className='flex flex-col gap-1 border-b border-border/60 px-3 py-2 text-sm last:border-b-0'
          >
            <div className='flex min-w-0 items-center gap-2'>
              <Badge variant='outline'>Reset</Badge>
              <span className='shrink-0 font-mono text-xs font-semibold'>{r.method}</span>
              <span className='min-w-0 flex-1 break-all font-mono text-xs'>{r.url}</span>
            </div>
            {r.fields.length > 0 && (
              <div className='flex flex-wrap gap-1'>
                {r.fields.map((f) => (
                  <Badge key={f} variant='secondary'>
                    {fieldLabel(f)}
                  </Badge>
                ))}
              </div>
            )}
            <div className='flex flex-wrap items-center gap-2'>
              <span className='text-xs text-muted-foreground'>
                {`Saved ${formatRelativeTime(r.savedAt)}`}
              </span>
              <Button
                variant='outline'
                size='sm'
                className='h-7'
                disabled={busy}
                onClick={() => onRestore(r)}
              >
                Restore my version
              </Button>
              <Button
                variant='ghost'
                size='sm'
                className='h-7'
                disabled={busy}
                onClick={() => onDiscard(r)}
              >
                Discard backup
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** What the user changed against the last synced spec, with a way to reset it to the spec. */
export function OpenApiSyncChanges({ collection }: { collection: string }) {
  const changes = useOpenApiSyncChanges(collection);
  const reset = useResetOpenApiSync(collection);
  const restore = useRestoreOpenApiSync(collection);
  const discard = useDiscardOpenApiBackup(collection);
  const [pending, setPending] = useState<PendingReset | null>(null);
  const [discarding, setDiscarding] = useState<OpenApiBackupRow | null>(null);
  if (changes.isError) {
    return <StateBlock tone='error'>{syncErrorMessage(changes.error)}</StateBlock>;
  }
  const c = changes.data;
  if (!c) return <StateBlock tone='loading'>Loading your changes...</StateBlock>;

  const total = c.modified.length + c.added.length + c.deleted.length + c.backups.length;
  if (total === 0)
    return (
      <StateBlock tone='empty'>
        You have not changed anything. Edit a synced request and it shows up here with the values
        you changed.
      </StateBlock>
    );
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
      {restore.isError && <StateBlock tone='error'>{syncErrorMessage(restore.error)}</StateBlock>}
      {discard.isError && <StateBlock tone='error'>{syncErrorMessage(discard.error)}</StateBlock>}
      <Group
        collection={collection}
        kind='modified'
        rows={c.modified}
        showFields={c.baselineTrusted}
        reset={control}
        onResetAll={resetAll}
      />
      <BackupGroup
        rows={c.backups}
        busy={restore.isPending || discard.isPending}
        onRestore={(row) => {
          restore.reset();
          restore.mutate(row.key, { onSuccess: () => toast.success('Your version is back.') });
        }}
        onDiscard={(row) => {
          discard.reset();
          setDiscarding(row);
        }}
      />
      <Group collection={collection} kind='added' rows={c.added} showFields={false} />
      <Group collection={collection} kind='deleted' rows={c.deleted} showFields={false} />
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
          if (pending) {
            reset.mutate(pending.keys, {
              onSuccess: ({ reset: done, skipped }) => {
                if (done.length > 0) {
                  toast.success(
                    `Reset ${done.length} ${done.length === 1 ? 'request' : 'requests'}.`,
                  );
                }
                if (skipped.length > 0) {
                  toast.info(
                    `${skipped.length} ${skipped.length === 1 ? 'request was' : 'requests were'} already like the spec or could not be reset.`,
                  );
                }
              },
            });
          }
          setPending(null);
        }}
      />
      <ResetConfirmDialog
        open={discarding !== null}
        onOpenChange={(open) => {
          if (!open) setDiscarding(null);
        }}
        title='Discard this backup?'
        body={
          discarding
            ? `Discard the backup of ${discarding.method} ${discarding.url}? You cannot restore it afterwards.`
            : ''
        }
        confirmLabel='Discard'
        onConfirm={() => {
          if (discarding) discard.mutate(discarding.key);
          setDiscarding(null);
        }}
      />
    </div>
  );
}
