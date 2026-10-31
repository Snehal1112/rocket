import { Search } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { StateBlock } from '@/components/ui/state-block';
import {
  useDeleteRequests,
  useDiscardOpenApiBackup,
  useOpenApiSyncChanges,
  useResetOpenApiSync,
  useRestoreOpenApiSync,
} from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiBackupRow, OpenApiChangeRow } from '@/lib/tauri-api';
import { closeDeletedRequestTabs } from './closeDeletedRequestTabs';
import { type DeleteRequestRow, DeleteRequestsDialog } from './DeleteRequestsDialog';
import {
  fieldLabel,
  isResetEligible,
  resetAllConfirmText,
  resetConfirmText,
} from './openApiSyncView';
import { openSyncRequest } from './openSyncRequest';
import { ResetConfirmDialog } from './ResetConfirmDialog';
import { MethodPill, SyncBadge } from './SyncBadge';
import { syncErrorMessage } from './syncErrors';
import { ValueBox } from './ValueBox';

type Kind = 'modified' | 'added' | 'deleted';

/** The filter shows once there are more rows than this, so a short list stays plain. */
const SEARCH_FROM = 8;

const GROUP_TITLE: Record<Kind, string> = {
  modified: 'Modified',
  added: 'Added by you',
  deleted: 'Deleted by you',
};

interface ResetControl {
  enabled: boolean;
  onReset: (row: OpenApiChangeRow) => void;
}

interface DeleteControl {
  enabled: boolean;
  onDelete: (row: OpenApiChangeRow) => void;
}

function ChangeRow({
  collection,
  row,
  kind,
  showFields,
  reset,
  del,
}: {
  collection: string;
  row: OpenApiChangeRow;
  kind: Kind;
  showFields: boolean;
  reset?: ResetControl;
  del?: DeleteControl;
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
  const showChips = showFields && kind === 'modified' && row.fields.length > 0;
  const actionClass =
    'h-6 shrink-0 px-2 text-xs opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100';
  return (
    <li className='group border-b border-border px-3 py-2 text-sm last:border-b-0 hover:bg-muted/30'>
      <div className='flex min-h-6 min-w-0 flex-wrap items-center gap-x-3 gap-y-1'>
        {row.method && <MethodPill method={row.method} />}
        <span
          title={target}
          className='min-w-0 max-w-[min(100%,36rem)] shrink truncate font-mono text-xs font-medium'
        >
          {target}
        </span>
        {showChips && (
          <div className='flex min-w-0 flex-wrap items-center gap-1'>
            {row.fields.map((f) => (
              <SyncBadge key={f} tone='info' className='font-normal'>
                {fieldLabel(f)}
              </SyncBadge>
            ))}
          </div>
        )}
        <div className='ml-auto flex shrink-0 items-center gap-1'>
          {canExpand && (
            <Button
              variant='link'
              size='sm'
              className='h-6 p-0 text-xs'
              aria-expanded={open}
              onClick={() => setOpen((v) => !v)}
            >
              {open ? 'Hide changes' : 'Show changes'}
            </Button>
          )}
          {canOpen && (
            <Button
              variant='ghost'
              size='sm'
              className={actionClass}
              onClick={() => void openRequest()}
            >
              Open request
            </Button>
          )}
          {kind === 'added' && del && row.requestPath !== null && (
            <Button
              variant='ghost'
              size='sm'
              className={actionClass}
              disabled={!del.enabled}
              onClick={() => del.onDelete(row)}
            >
              Delete
            </Button>
          )}
          {kind === 'modified' && reset && row.key && isResetEligible(row.fields) && (
            <Button
              variant='ghost'
              size='sm'
              className={actionClass}
              disabled={!reset.enabled}
              onClick={() => reset.onReset(row)}
            >
              Reset to spec
            </Button>
          )}
        </div>
      </div>
      {missing && (
        <p role='alert' className='text-xs text-destructive'>
          The request file was not found.
        </p>
      )}
      {canExpand && open && (
        <div className='flex flex-col gap-2 py-2 pl-[5.5rem]'>
          {row.changes.map((c) => (
            <div key={c.field} className='flex flex-col gap-1 rounded-sm border border-border p-2'>
              <span className='break-all text-xs font-medium'>{fieldLabel(c.field)}</span>
              <div className='grid gap-2 md:grid-cols-2'>
                <ValueBox title='Baseline' value={c.baseline} />
                <ValueBox title='Yours' value={c.yours} />
              </div>
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
  total,
  showFields,
  reset,
  onResetAll,
  del,
  onDeleteAll,
}: {
  collection: string;
  kind: Kind;
  /** The rows to show. A filter can make this fewer than `total`. */
  rows: OpenApiChangeRow[];
  total: number;
  showFields: boolean;
  reset?: ResetControl;
  onResetAll?: () => void;
  del?: DeleteControl;
  onDeleteAll?: () => void;
}) {
  if (rows.length === 0) return null;
  // A bulk action always covers the whole group, so it hides while a filter narrows the rows.
  const filtering = rows.length !== total;
  const resettable = rows.filter((r) => r.key && isResetEligible(r.fields)).length;
  return (
    <section className='flex flex-col'>
      <div className='sticky top-0 z-10 mb-1 flex min-h-11 items-center justify-between gap-3 rounded-lg border border-border bg-card px-3 py-2 shadow-sm'>
        <h3 className='min-w-0 truncate text-sm font-semibold'>
          {filtering
            ? `${GROUP_TITLE[kind]} (${rows.length} of ${total})`
            : `${GROUP_TITLE[kind]} (${rows.length})`}
        </h3>
        {!filtering && onResetAll && reset && resettable > 0 && (
          <Button variant='outline' size='sm' disabled={!reset.enabled} onClick={onResetAll}>
            {`Reset all to spec (${resettable})`}
          </Button>
        )}
        {!filtering && onDeleteAll && del && (
          <Button variant='outline' size='sm' disabled={!del.enabled} onClick={onDeleteAll}>
            {`Delete all (${rows.length})`}
          </Button>
        )}
      </div>
      <ul>
        {rows.map((r) => (
          <ChangeRow
            key={`${r.uid}:${r.requestPath}`}
            collection={collection}
            row={r}
            kind={kind}
            showFields={showFields}
            reset={reset}
            del={del}
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
    <section className='flex flex-col'>
      <div className='mb-1 flex min-h-11 items-center rounded-lg border border-border bg-card px-3 py-2 shadow-sm'>
        <h3 className='min-w-0 truncate text-sm font-semibold'>{`Reset by you (${rows.length})`}</h3>
      </div>
      <ul className='flex flex-col'>
        {rows.map((r) => (
          <li
            key={r.key}
            className='flex flex-col gap-2 border-b border-border px-3 py-3 text-sm last:border-b-0 hover:bg-muted/30'
          >
            <div className='flex min-w-0 items-center gap-3'>
              <MethodPill method={r.method} />
              <span className='min-w-0 flex-1 break-all font-mono text-xs font-medium'>
                {r.url}
              </span>
            </div>
            {r.fields.length > 0 && (
              <div className='flex flex-wrap items-center gap-1 pl-[5.5rem]'>
                {r.fields.map((f) => (
                  <SyncBadge key={f} tone='info' className='font-normal'>
                    {fieldLabel(f)}
                  </SyncBadge>
                ))}
              </div>
            )}
            <div className='flex flex-wrap items-center justify-between gap-x-3 gap-y-1 pl-[5.5rem]'>
              <span className='text-xs text-muted-foreground'>
                {`Saved ${formatRelativeTime(r.savedAt)}`}
              </span>
              <div className='flex items-center gap-1'>
                <Button
                  variant='ghost'
                  size='sm'
                  className='h-7 text-xs'
                  disabled={busy}
                  onClick={() => onDiscard(r)}
                >
                  Discard backup
                </Button>
                <Button
                  variant='outline'
                  size='sm'
                  className='h-7 text-xs'
                  disabled={busy}
                  onClick={() => onRestore(r)}
                >
                  Restore my version
                </Button>
              </div>
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
  const del = useDeleteRequests(collection);
  const [deleting, setDeleting] = useState<DeleteRequestRow[] | null>(null);
  const [pending, setPending] = useState<PendingReset | null>(null);
  const [discarding, setDiscarding] = useState<OpenApiBackupRow | null>(null);
  const [query, setQuery] = useState('');
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
  const deleteRow = (row: OpenApiChangeRow): DeleteRequestRow[] =>
    row.requestPath === null
      ? []
      : [
          {
            uid: row.uid,
            requestPath: row.requestPath,
            label: `${row.method} ${row.url}`.trim() || row.name,
          },
        ];
  const deleteControl: DeleteControl = {
    enabled: !del.isPending,
    onDelete: (row) => {
      del.reset();
      setDeleting(deleteRow(row));
    },
  };
  const deleteAll = () => {
    del.reset();
    setDeleting(c.added.flatMap(deleteRow));
  };
  const resetAll = () => {
    const keys = c.modified.flatMap((r) => (r.key && isResetEligible(r.fields) ? [r.key] : []));
    reset.reset();
    setPending({ keys, body: resetAllConfirmText(keys.length) });
  };
  const needle = query.trim().toLowerCase();
  const narrow = (rows: OpenApiChangeRow[]) =>
    needle
      ? rows.filter((r) =>
          `${r.method} ${r.url} ${r.requestPath ?? ''} ${r.name}`.toLowerCase().includes(needle),
        )
      : rows;
  const modified = narrow(c.modified);
  const added = narrow(c.added);
  const deleted = narrow(c.deleted);
  // The box stays while a filter is on, so a shrinking list never traps the user in a filter.
  const searchable =
    c.modified.length + c.added.length + c.deleted.length > SEARCH_FROM || needle !== '';
  const noMatch = needle !== '' && modified.length + added.length + deleted.length === 0;
  return (
    <div className='flex flex-col gap-3'>
      <p className='text-xs text-muted-foreground'>
        Differences between your collection and the last synced spec. Sync never overwrites these
        without asking.
      </p>
      {searchable && (
        <div className='relative max-w-md'>
          <Search
            aria-hidden='true'
            className='pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground'
          />
          {/* Same look as the other filter fields. The native search styling is switched off. */}
          <Input
            type='search'
            aria-label='Filter your changes'
            placeholder='Filter by method, path or name'
            className='h-7 appearance-none pl-7 text-sm [&::-webkit-search-cancel-button]:appearance-none'
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      )}
      {noMatch && <StateBlock tone='empty'>{`No changes match "${query.trim()}".`}</StateBlock>}
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
        rows={modified}
        total={c.modified.length}
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
      {del.isError && deleting === null && (
        <StateBlock tone='error'>{syncErrorMessage(del.error)}</StateBlock>
      )}
      <Group
        collection={collection}
        kind='added'
        rows={added}
        total={c.added.length}
        showFields={false}
        del={deleteControl}
        onDeleteAll={deleteAll}
      />
      <Group
        collection={collection}
        kind='deleted'
        rows={deleted}
        total={c.deleted.length}
        showFields={false}
      />
      <p className='text-xs text-muted-foreground'>
        Scripts, tests and assertions are not tracked here.
      </p>
      <DeleteRequestsDialog
        requests={deleting ?? []}
        open={deleting !== null}
        busy={del.isPending}
        error={del.isError ? syncErrorMessage(del.error) : null}
        onCancel={() => setDeleting(null)}
        onConfirm={() => {
          if (!deleting) return;
          del.mutate(
            deleting.map(({ uid, requestPath }) => ({ uid, requestPath })),
            {
              onSuccess: () => {
                closeDeletedRequestTabs(
                  collection,
                  deleting.map((r) => r.requestPath),
                );
                toast.success('Deleted.');
                setDeleting(null);
              },
            },
          );
        }}
      />
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
