import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { OpenApiFieldChange, OpenApiSyncPlan, OpenApiVariableChange } from '@/lib/tauri-api';
import { fieldLabel, findEntry, outcomeLabel, renameHint, variableLabel } from './specSyncView';

interface SpecSyncDetailProps {
  plan: OpenApiSyncPlan;
  selectedId: string | null;
}

function Value({ title, value }: { title: string; value?: string }) {
  return (
    <div className='min-w-0'>
      <p className='text-xs font-medium text-muted-foreground'>{title}</p>
      <pre className='whitespace-pre-wrap break-all rounded-md border bg-muted/40 p-2 font-mono text-xs'>
        {value === undefined ? '(none)' : value}
      </pre>
    </div>
  );
}

function outcomeVariant(outcome: OpenApiFieldChange['outcome']) {
  if (outcome === 'conflict') return 'warning' as const;
  return outcome === 'takeSpec' ? ('default' as const) : ('secondary' as const);
}

function FieldBlock({ change }: { change: OpenApiFieldChange }) {
  return (
    <div className='flex flex-col gap-2 rounded-md border p-3'>
      <div className='flex items-center justify-between gap-2'>
        <span className='break-all text-sm font-medium'>{fieldLabel(change.field)}</span>
        <Badge variant={outcomeVariant(change.outcome)}>{outcomeLabel(change.outcome)}</Badge>
      </div>
      <Value title='Baseline' value={change.baseline} />
      <Value title='Yours' value={change.yours} />
      <Value title='Spec' value={change.spec} />
    </div>
  );
}

function VariableBlock({ change }: { change: OpenApiVariableChange }) {
  return (
    <div className='flex flex-col gap-2 rounded-md border p-3'>
      <span className='break-all text-sm font-medium'>{variableLabel(change)}</span>
      <Value title='Baseline' value={change.baseline} />
      <Value title='Yours' value={change.yours} />
      <Value title='Spec' value={change.spec} />
    </div>
  );
}

/** The detail of the selected entry. Only this one entry is rendered. */
export function SpecSyncDetail({ plan, selectedId }: SpecSyncDetailProps) {
  const entry = findEntry(plan, selectedId);
  return (
    <ScrollArea className='h-full'>
      <div className='flex flex-col gap-3 p-4'>
        {plan.cautious && (
          <p role='alert' className='rounded-md border border-warning/50 p-2 text-xs'>
            Rocket could not tell for sure which of your edits are new, so it plays safe and keeps
            what you have wherever it is unsure.
          </p>
        )}
        {plan.warnings.map((w) => (
          <p key={w} className='rounded-md border p-2 text-xs text-muted-foreground'>
            {w}
          </p>
        ))}
        {!entry && <p className='text-sm text-muted-foreground'>Select a change to see it.</p>}
        {entry?.type === 'variable' && <VariableBlock change={entry.change} />}
        {entry?.type === 'item' && (
          <>
            <div className='flex flex-col gap-1'>
              <h2 className='break-all text-base font-semibold'>{entry.item.name}</h2>
              {renameHint(entry.item) && (
                <p className='text-xs text-muted-foreground'>{renameHint(entry.item)}</p>
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
              </div>
            )}
            {entry.item.kind === 'changed' && (
              <>
                {entry.item.cautious && (
                  <p className='text-xs text-muted-foreground'>
                    This request is handled with extra care.
                  </p>
                )}
                {entry.item.changes.map((c) => (
                  <FieldBlock key={c.field} change={c} />
                ))}
              </>
            )}
            {entry.item.kind === 'removed' && (
              <div className='flex flex-col gap-1 text-sm'>
                <p className='text-muted-foreground'>
                  The spec no longer has <span className='font-mono'>{entry.item.key}</span>.
                </p>
                {entry.item.edited === null && <p>Treated as edited, will be kept</p>}
                {entry.item.edited === true && <p>You edited it, so it will be kept.</p>}
                {entry.item.edited === false && <p>You did not edit it.</p>}
                <p className='break-all text-xs text-muted-foreground'>{entry.item.requestPath}</p>
              </div>
            )}
          </>
        )}
      </div>
    </ScrollArea>
  );
}
