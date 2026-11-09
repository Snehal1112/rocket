import { GitCompare, KeyRound, ShieldAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { SyncConflictDiff } from '@/components/network/SyncConflictDiff';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { StateBlock } from '@/components/ui/state-block';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import {
  conflictKindText,
  keepBothAllowed,
  SAFE_SANDBOX_NOTE,
  syncCountsText,
} from '@/lib/network-sync-utils';
import { formatBytes, peerName } from '@/lib/network-utils';
import {
  type ConflictView,
  confirmNetworkSync,
  reviewNetworkSync,
  type SyncResolution,
} from '@/lib/tauri-api';
import { type SyncReviewEntry, useNetworkStore } from '@/stores/network-store';

const store = () => useNetworkStore.getState();

const RESOLUTION_LABELS: { value: SyncResolution; label: string }[] = [
  { value: 'keep_mine', label: 'Keep mine' },
  { value: 'keep_theirs', label: 'Keep theirs' },
  { value: 'keep_both', label: 'Keep both' },
];

function PathMarks({
  icon,
  heading,
  paths,
}: {
  icon: 'script' | 'auth';
  heading: string;
  paths: string[];
}) {
  const Icon = icon === 'script' ? ShieldAlert : KeyRound;
  return (
    <div className='space-y-1'>
      <p className='flex items-center gap-1.5 text-xs font-medium'>
        <Icon className='h-3.5 w-3.5' aria-hidden='true' />
        {heading}
      </p>
      <ul className='space-y-0.5 pl-5'>
        {paths.map((path) => (
          <li key={path} className='break-all font-mono text-xs'>
            {path}
          </li>
        ))}
      </ul>
    </div>
  );
}

function ConflictRow({
  entry,
  conflict,
  choice,
  comparing,
  name,
  onChoose,
  onToggle,
}: {
  entry: SyncReviewEntry;
  conflict: ConflictView;
  choice: SyncResolution | undefined;
  comparing: boolean;
  name: string;
  onChoose: (value: SyncResolution) => void;
  onToggle: () => void;
}) {
  const { path } = conflict;
  const both = conflict.mineHash !== null && conflict.theirsHash !== null && keepBothAllowed(path);
  const idBase = `sync-${encodeURIComponent(path)}`;
  return (
    <li className='space-y-2 rounded-md border p-3'>
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <p className='break-all font-mono text-xs'>{path}</p>
          <p className='text-2xs text-muted-foreground'>{conflictKindText(conflict.kind)}</p>
        </div>
        <Button
          variant='ghost'
          size='sm'
          aria-label={comparing ? `Hide ${path}` : `Compare ${path}`}
          onClick={onToggle}
        >
          <GitCompare className='h-4 w-4' aria-hidden='true' />
        </Button>
      </div>
      <RadioGroup
        aria-label={`Resolution for ${path}`}
        value={choice ?? ''}
        onValueChange={(v) => onChoose(v as SyncResolution)}
        className='flex flex-wrap gap-4'
      >
        {RESOLUTION_LABELS.filter((r) => r.value !== 'keep_both' || both).map((r) => (
          <div key={r.value} className='flex items-center gap-2'>
            <RadioGroupItem value={r.value} id={`${idBase}-${r.value}`} />
            <Label htmlFor={`${idBase}-${r.value}`} className='text-sm font-normal'>
              {r.label}
            </Label>
          </div>
        ))}
      </RadioGroup>
      {comparing && <SyncConflictDiff syncId={entry.syncId} conflict={conflict} peerName={name} />}
    </li>
  );
}

/**
 * The dialog of one sync: the review of a sync another node started, the confirm of one this node
 * started, and the running view after the answer. Nothing is saved before the user answers.
 * Mount once.
 */
export function SyncReviewDialog() {
  const id = useNetworkStore((s) => s.dialogSyncId);
  const entry = useNetworkStore((s) => s.syncReviews.find((r) => r.syncId === s.dialogSyncId));
  const progress = useNetworkStore((s) => s.syncProgress.find((p) => p.syncId === s.dialogSyncId));
  const endedText = useNetworkStore((s) => s.dialogSyncEndedText);
  const [choices, setChoices] = useState<Record<string, SyncResolution>>({});
  const [comparing, setComparing] = useState<string | null>(null);
  const { busy, error, run, reset } = useAsyncAction('inline');

  // biome-ignore lint/correctness/useExhaustiveDependencies: the dialog starts clean for each sync.
  useEffect(() => {
    setChoices({});
    setComparing(null);
    reset();
  }, [id, reset]);

  if (!id) return null;

  const close = () => store().closeSyncDialog();

  if (!entry) {
    return (
      <Dialog
        open
        onOpenChange={(next) => {
          if (!next) close();
        }}
      >
        <DialogContent className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle>Sync</DialogTitle>
            <DialogDescription>{endedText ?? 'The request expired.'}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button onClick={close}>Done</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }

  const name = peerName(entry.displayName);
  const isReview = entry.kind === 'review';
  const { summary } = entry;
  const changes = summary.addedHere + summary.changedHere + summary.deletedHere;
  const showConflicts = isReview && !entry.answered && entry.conflicts.length > 0;
  const allChosen = entry.conflicts.every((c) => choices[c.path] !== undefined);

  let title: string;
  let description: string;
  if (entry.answered) {
    title = `Syncing with ${name}`;
    description = 'You can close this window. The sync goes on.';
  } else if (isReview) {
    title = `${name} wants to sync a collection`;
    description = 'Nothing is saved until you accept.';
  } else {
    const sent = summary.sentAdded + summary.sentChanged + summary.sentDeleted;
    title =
      changes === 0
        ? `Send ${sent} ${sent === 1 ? 'change' : 'changes'} to ${name}`
        : `Apply ${changes} ${changes === 1 ? 'change' : 'changes'} from ${name}`;
    description = 'Nothing is saved until you apply.';
  }

  async function accept() {
    const decisions = entry?.conflicts.map((c) => ({ path: c.path, resolution: choices[c.path] }));
    const ok = await run(async () => {
      if (isReview) await reviewNetworkSync(id as string, true, decisions ?? []);
      else await confirmNetworkSync(id as string, true);
    });
    if (ok) store().markSyncAnswered(id as string);
  }

  async function decline() {
    const syncId = id as string;
    // The finish that follows is the user's own doing, so the bridge shows no toast for it.
    store().markSyncDeclinedHere(syncId);
    const ok = await run(async () => {
      try {
        if (isReview) await reviewNetworkSync(syncId, false, []);
        else await confirmNetworkSync(syncId, false);
      } catch (err) {
        store().unmarkSyncDeclinedHere(syncId);
        throw err;
      }
    });
    if (!ok) return;
    store().removeSyncReview(syncId);
    if (store().dialogSyncId === syncId) store().closeSyncDialog();
  }

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        // An unanswered entry stays pending, and its toast comes back through the hook.
        if (!next && !busy) close();
      }}
    >
      <DialogContent className='max-h-[90vh] sm:max-w-2xl'>
        <DialogHeader>
          <DialogTitle className='break-words'>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <div className='max-h-[80vh] space-y-4 overflow-y-auto'>
          {entry.answered ? (
            <>
              <p role='status' className='text-sm'>{`Syncing with ${name}...`}</p>
              {progress && (
                <p role='status' className='text-xs text-muted-foreground'>
                  {`${formatBytes(progress.doneBytes)} of ${formatBytes(progress.totalBytes)}`}
                </p>
              )}
            </>
          ) : (
            <>
              <div className='space-y-1'>
                <p className='break-words text-sm font-medium'>{entry.collectionName}</p>
                <p className='font-mono text-xs text-muted-foreground'>{`Device ID ${entry.shortId}`}</p>
                <p className='text-xs'>
                  {`Coming to you: ${syncCountsText(summary.addedHere, summary.changedHere, summary.deletedHere)}`}
                </p>
                <p className='text-xs'>
                  {`Going to ${name}: ${syncCountsText(summary.sentAdded, summary.sentChanged, summary.sentDeleted)}`}
                </p>
              </div>
              {summary.scriptsChanged.length > 0 && (
                <PathMarks
                  icon='script'
                  heading={`Scripts changed (${summary.scriptsChanged.length})`}
                  paths={summary.scriptsChanged}
                />
              )}
              {summary.authChanged.length > 0 && (
                <PathMarks
                  icon='auth'
                  heading={`Auth changed (${summary.authChanged.length})`}
                  paths={summary.authChanged}
                />
              )}
              <p
                role='note'
                className='flex items-start gap-2 rounded-md border px-3 py-2 text-xs text-muted-foreground'
              >
                <ShieldAlert className='mt-0.5 h-3.5 w-3.5 shrink-0' aria-hidden='true' />
                {SAFE_SANDBOX_NOTE}
              </p>
              {showConflicts && (
                <div className='space-y-2'>
                  <p className='text-sm font-medium'>{`Conflicts (${entry.conflicts.length})`}</p>
                  <p className='text-xs text-muted-foreground'>
                    Choose what to keep for each one. Nothing changes until you press Accept.
                  </p>
                  <ul className='space-y-2'>
                    {entry.conflicts.map((conflict) => (
                      <ConflictRow
                        key={conflict.path}
                        entry={entry}
                        conflict={conflict}
                        choice={choices[conflict.path]}
                        comparing={comparing === conflict.path}
                        name={name}
                        onChoose={(value) => setChoices((c) => ({ ...c, [conflict.path]: value }))}
                        onToggle={() =>
                          setComparing((cur) => (cur === conflict.path ? null : conflict.path))
                        }
                      />
                    ))}
                  </ul>
                </div>
              )}
            </>
          )}
          {error && <StateBlock tone='error'>{error}</StateBlock>}
        </div>
        <DialogFooter>
          {entry.answered ? (
            <Button onClick={close}>Done</Button>
          ) : (
            <>
              <Button variant='outline' disabled={busy} onClick={() => void decline()}>
                Decline
              </Button>
              <Button disabled={busy || (isReview && !allChosen)} onClick={() => void accept()}>
                {isReview ? 'Accept' : 'Apply'}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
