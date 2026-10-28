import { Link2, Unlink } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { SegmentedControl } from '@/components/ui/segmented-control';
import { useDeleteRequests } from '@/lib/queries/openapi-sync-queries';
import type {
  OpenApiFieldChange,
  OpenApiNewPreview,
  OpenApiSyncItem,
  OpenApiSyncPlan,
  OpenApiVariableChange,
} from '@/lib/tauri-api';
import { closeDeletedRequestTabs } from './closeDeletedRequestTabs';
import { DeleteRequestsDialog } from './DeleteRequestsDialog';
import {
  defaultChoice,
  fieldLabel,
  findEntry,
  outcomeLabel,
  renameHint,
  variableLabel,
} from './openApiSyncView';
import { MethodPill, SyncBadge } from './SyncBadge';
import { syncErrorMessage } from './syncErrors';
import { ValueBox } from './ValueBox';

type Choice = 'spec' | 'yours';

export interface OpenApiSyncDetailProps {
  collection: string;
  plan: OpenApiSyncPlan;
  selectedId: string | null;
  /** True inside an open row: no scroll area, no plan notices and no repeated title. */
  inline?: boolean;
  /** The explicit field choices by item key. A field without one takes its default. */
  choices: Map<string, Record<string, Choice>>;
  /** The rename links in use, as `[oldKey, newKey]` pairs. */
  links: Array<[string, string]>;
  onChoose: (key: string, field: string, choice: Choice) => void;
  onLink: (oldKey: string, newKey: string) => void;
  onUnlink: (oldKey: string) => void;
  /** True while the plan is a placeholder, so a link would act on stale rows. */
  linksLocked: boolean;
}

const CHOICE_OPTIONS: { value: Choice; label: string }[] = [
  { value: 'spec', label: 'Take spec' },
  { value: 'yours', label: 'Keep yours' },
];

/** Deletes a request the spec removed and the user edited, after asking. */
function DeleteRemovedRequest({
  collection,
  uid,
  requestPath,
  label,
}: {
  collection: string;
  uid: string;
  requestPath: string;
  label: string;
}) {
  const del = useDeleteRequests(collection);
  const [asking, setAsking] = useState(false);
  return (
    <div className='flex flex-col gap-1'>
      <div>
        <Button
          variant='outline'
          size='sm'
          disabled={del.isPending}
          onClick={() => {
            del.reset();
            setAsking(true);
          }}
        >
          Delete this request
        </Button>
      </div>
      {del.isError && !asking && (
        <p role='alert' className='text-xs text-destructive'>
          {syncErrorMessage(del.error)}
        </p>
      )}
      <DeleteRequestsDialog
        requests={[{ uid, requestPath, label }]}
        open={asking}
        busy={del.isPending}
        error={del.isError ? syncErrorMessage(del.error) : null}
        onCancel={() => setAsking(false)}
        onConfirm={() => {
          del.mutate([{ uid, requestPath }], {
            onSuccess: () => {
              closeDeletedRequestTabs(collection, [requestPath]);
              toast.success('Deleted.');
              setAsking(false);
            },
          });
        }}
      />
    </div>
  );
}

function NewPreview({ preview }: { preview: OpenApiNewPreview }) {
  const rows: Array<[string, string]> = [
    ['Headers', preview.headers.length > 0 ? preview.headers.join(', ') : 'None'],
    ['Query', preview.query.length > 0 ? preview.query.join(', ') : 'None'],
    ['Body', preview.hasBody ? 'Yes' : 'None'],
    ['Auth', preview.authKind ?? 'None of its own'],
  ];
  return (
    <div className='flex flex-col gap-2 rounded-md border bg-card p-3 text-xs'>
      <p className='flex min-w-0 items-center gap-2'>
        <MethodPill method={preview.method} />
        <span className='min-w-0 break-all font-mono'>{preview.url}</span>
      </p>
      <dl className='grid grid-cols-[5rem_1fr] gap-x-3 gap-y-1'>
        {rows.map(([label, value]) => (
          <div key={label} className='contents'>
            <dt className='text-muted-foreground'>{label}</dt>
            <dd className='min-w-0 break-all'>{value}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function outcomeTone(outcome: OpenApiFieldChange['outcome']) {
  if (outcome === 'conflict') return 'warning' as const;
  return outcome === 'takeSpec' ? ('info' as const) : ('neutral' as const);
}

function FieldBlock({
  change,
  value,
  onChoose,
}: {
  change: OpenApiFieldChange;
  value: Choice;
  onChoose: (choice: Choice) => void;
}) {
  return (
    <div className='flex flex-col gap-2 rounded-md border p-3'>
      <div className='flex items-center justify-between gap-2'>
        <span className='break-all text-sm font-medium'>{fieldLabel(change.field)}</span>
        <SyncBadge tone={outcomeTone(change.outcome)}>{outcomeLabel(change.outcome)}</SyncBadge>
      </div>
      <div className='grid gap-2 md:grid-cols-3'>
        <ValueBox title='Baseline' value={change.baseline} />
        <ValueBox title='Yours' value={change.yours} />
        <ValueBox title='Spec' value={change.spec} />
      </div>
      <SegmentedControl
        aria-label={fieldLabel(change.field)}
        value={value}
        onValueChange={onChoose}
        options={CHOICE_OPTIONS}
      />
    </div>
  );
}

function VariableBlock({ change }: { change: OpenApiVariableChange }) {
  return (
    <div className='flex flex-col gap-2 rounded-md border p-3'>
      <span className='break-all text-sm font-medium'>{variableLabel(change)}</span>
      <ValueBox title='Baseline' value={change.baseline} />
      <ValueBox title='Yours' value={change.yours} />
      <ValueBox title='Spec' value={change.spec} />
      <p className='text-xs text-muted-foreground'>
        {change.kind === 'baseUrl'
          ? 'If you changed this value yourself, Rocket keeps yours and only updates what it remembers of the spec. With no trusted baseline the value is not updated.'
          : 'Applying adds this variable if you do not have it yet.'}
      </p>
    </div>
  );
}

/** The plan-level notes: the cautious mode and the warnings. Shown once above the sections. */
export function PlanNotices({ plan }: { plan: OpenApiSyncPlan }) {
  if (!plan.cautious && plan.warnings.length === 0) return null;
  return (
    <div className='flex flex-col gap-2'>
      {plan.cautious && (
        <p role='alert' className='rounded-md border border-warning/50 p-2 text-xs'>
          Rocket could not tell for sure which of your edits are new, so it plays safe and keeps
          what you have wherever it is unsure.
        </p>
      )}
      {plan.warnings.map((w, i) => (
        // Warnings can repeat, so the index keeps the keys unique.
        // biome-ignore lint/suspicious/noArrayIndexKey: the list is static and read-only.
        <p key={i} className='rounded-md border p-2 text-xs text-muted-foreground'>
          {w}
        </p>
      ))}
    </div>
  );
}

/** The detail of the selected entry. Only this one entry is rendered. */
export function OpenApiSyncDetail({
  collection,
  plan,
  selectedId,
  choices,
  links,
  onChoose,
  onLink,
  onUnlink,
  linksLocked,
  inline = false,
}: OpenApiSyncDetailProps) {
  const entry = findEntry(plan, selectedId);
  // The pair `[oldKey, newKey]` a rename hint points at, or null without a hint.
  const renameLink = (item: OpenApiSyncItem): [string, string] | null => {
    if (item.kind === 'new' && item.possiblyRenamedFrom) {
      return [item.possiblyRenamedFrom, item.key];
    }
    if (item.kind === 'removed' && item.possiblyRenamedTo) {
      return [item.key, item.possiblyRenamedTo];
    }
    return null;
  };
  const linkedFrom = (key: string): string | null => links.find(([, n]) => n === key)?.[0] ?? null;
  const body = (
    <div className={inline ? 'flex flex-col gap-3' : 'flex flex-col gap-3 p-4'}>
      {!inline && <PlanNotices plan={plan} />}
      {!entry && <p className='text-sm text-muted-foreground'>Select a change to see it.</p>}
      {entry?.type === 'variable' && <VariableBlock change={entry.change} />}
      {entry?.type === 'item' && (
        <>
          <div className='flex flex-col gap-1'>
            {!inline && <h2 className='break-all text-base font-semibold'>{entry.item.name}</h2>}
            {renameHint(entry.item) && (
              <p className='text-xs text-muted-foreground'>{renameHint(entry.item)}</p>
            )}
            {renameLink(entry.item) && (
              <Button
                variant='outline'
                size='sm'
                className='w-fit'
                disabled={linksLocked}
                onClick={() => {
                  const pair = renameLink(entry.item);
                  if (pair) onLink(pair[0], pair[1]);
                }}
              >
                <Link2 />
                Link as rename
              </Button>
            )}
            {entry.item.kind === 'changed' && linkedFrom(entry.item.key) && (
              <Button
                variant='outline'
                size='sm'
                className='w-fit'
                disabled={linksLocked}
                onClick={() => {
                  const old = linkedFrom(entry.item.key);
                  if (old) onUnlink(old);
                }}
              >
                <Unlink />
                Unlink rename
              </Button>
            )}
          </div>
          {entry.item.kind === 'new' && (
            <div className='flex flex-col gap-1 text-sm'>
              <p>
                New request <span className='font-mono'>{entry.item.key}</span>
              </p>
              <p className='text-muted-foreground'>
                Folder:{' '}
                {entry.item.folder.length > 0 ? entry.item.folder.join(' / ') : 'collection root'}
              </p>
              <NewPreview preview={entry.item.preview} />
            </div>
          )}
          {entry.item.kind === 'changed' && (
            <>
              {entry.item.cautious && (
                <p className='text-xs text-muted-foreground'>
                  This request is handled with extra care.
                </p>
              )}
              <p className='break-all text-xs text-muted-foreground'>{entry.item.requestPath}</p>
              {entry.item.changes.map((c) => (
                <FieldBlock
                  key={c.field}
                  change={c}
                  value={choices.get(entry.item.key)?.[c.field] ?? defaultChoice(c.outcome)}
                  onChoose={(choice) => onChoose(entry.item.key, c.field, choice)}
                />
              ))}
            </>
          )}
          {entry.item.kind === 'removed' && (
            <div className='flex flex-col gap-1 text-sm'>
              <p className='text-muted-foreground'>
                The spec no longer has <span className='font-mono'>{entry.item.key}</span>.
              </p>
              {entry.item.edited === null && <p>Treated as edited, will be kept.</p>}
              {entry.item.edited === true && <p>You edited it, so it will be kept.</p>}
              {entry.item.edited === false && (
                <p>You did not edit it, so it will be deleted if Rocket can rewrite it.</p>
              )}
              <p className='break-all text-xs text-muted-foreground'>{entry.item.requestPath}</p>
              {entry.item.edited === true && (
                <DeleteRemovedRequest
                  key={entry.item.key}
                  collection={collection}
                  uid={entry.item.uid}
                  requestPath={entry.item.requestPath}
                  label={entry.item.name}
                />
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
  return inline ? body : <ScrollArea className='h-full'>{body}</ScrollArea>;
}
