import { Check, ChevronDown, ChevronRight, Loader2, X } from 'lucide-react';
import { lazy, Suspense, useEffect, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import {
  acceptProposal,
  dirtyAffectedTabPlace,
  rejectProposal,
} from '@/lib/assistant/proposal-actions';
import {
  loadProposalDiff,
  type ProposalDiff,
  proposalFailure,
  proposalPreview,
  proposalTarget,
} from '@/lib/assistant/proposal-view';
import type { AgentProposal } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

const ProposalDiffEditor = lazy(() =>
  import('./ProposalDiffEditor').then((m) => ({ default: m.ProposalDiffEditor })),
);

type BadgeVariant = 'default' | 'secondary' | 'outline' | 'warning' | 'destructive';

const STATUS_BADGE: Record<AgentProposal['status'], { label: string; variant: BadgeVariant }> = {
  pending: { label: 'Pending', variant: 'secondary' },
  accepted: { label: 'Accepted', variant: 'default' },
  rejected: { label: 'Rejected', variant: 'outline' },
  stale: { label: 'Stale', variant: 'warning' },
  failed: { label: 'Failed', variant: 'destructive' },
};

const GENERIC_ACTION_ERROR = 'The action failed. Try again.';

/** One proposed change with its preview and Accept and Reject. */
export function AssistantProposalCard({ proposal }: { proposal: AgentProposal }) {
  const target = proposalTarget(proposal.change);
  const preview = proposalPreview(proposal.change);
  const dirtyPlace = usePaneStore((s) =>
    proposal.status === 'pending' ? dirtyAffectedTabPlace(s, proposal.change) : undefined,
  );
  const blockedByEdits = dirtyPlace !== undefined;
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [diff, setDiff] = useState<ProposalDiff | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);
  const [diffFailed, setDiffFailed] = useState(false);
  const [retryCount, setRetryCount] = useState(0);
  const pending = proposal.status === 'pending';

  // Loads the diff only while the card is expanded and the proposal is
  // pending. After Accept the stored version already holds the change, so a
  // diff loaded earlier is kept and none is fetched later.
  // biome-ignore lint/correctness/useExhaustiveDependencies: retryCount re-runs the fetch on demand.
  useEffect(() => {
    if (preview.kind !== 'diff' || !expanded || !pending || diff) return;
    let cancelled = false;
    setDiffLoading(true);
    setDiffFailed(false);
    loadProposalDiff(proposal)
      .then((loaded) => {
        if (!cancelled) setDiff(loaded);
      })
      .catch((err) => {
        console.error('[assistant] failed to load the proposal diff', err);
        if (!cancelled) setDiffFailed(true);
      })
      .finally(() => {
        if (!cancelled) setDiffLoading(false);
      });
    return () => {
      cancelled = true;
      // The bail-out path would leave the flag set, so the effect resets it.
      setDiffLoading(false);
    };
  }, [proposal, preview.kind, expanded, pending, diff, retryCount]);

  const failure = proposalFailure(proposal);

  const run = async (action: (p: AgentProposal) => Promise<string | undefined>) => {
    setBusy(true);
    setActionError(null);
    setWarning(null);
    try {
      const note = await action(proposal);
      if (typeof note === 'string') setWarning(note);
    } catch (err) {
      console.error('[assistant] proposal action failed', err);
      setActionError(failure ? `${GENERIC_ACTION_ERROR} ${failure}` : GENERIC_ACTION_ERROR);
    } finally {
      setBusy(false);
    }
  };

  const badge = STATUS_BADGE[proposal.status];
  const Chevron = expanded ? ChevronDown : ChevronRight;

  return (
    <article
      aria-label={proposal.summary}
      className='flex flex-col gap-2 rounded-md border bg-card p-3'
    >
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <p className='text-sm font-medium'>{proposal.summary}</p>
          <p className='truncate text-xs text-muted-foreground'>
            {target.path ? `${target.collection} / ${target.path}` : target.collection}
          </p>
        </div>
        <Badge variant={badge.variant}>{badge.label}</Badge>
      </div>

      {preview.kind === 'diff' && (
        <Collapsible open={expanded} onOpenChange={setExpanded}>
          <CollapsibleTrigger asChild>
            <Button
              variant='ghost'
              size='sm'
              className='h-7 gap-1 px-1 text-xs'
              aria-expanded={expanded}
            >
              <Chevron className='h-3.5 w-3.5' aria-hidden='true' />
              {expanded ? 'Hide changes' : 'Show changes'}
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent className='pt-1'>
            {diff ? (
              <Suspense fallback={<EditorSkeleton />}>
                <ProposalDiffEditor
                  original={diff.before}
                  modified={diff.after}
                  language={diff.language}
                />
              </Suspense>
            ) : diffFailed ? (
              <div className='flex items-center gap-2 text-xs text-destructive'>
                <span>Could not load the current version.</span>
                <Button
                  variant='outline'
                  size='sm'
                  className='h-6 text-xs'
                  onClick={() => setRetryCount((n) => n + 1)}
                >
                  Retry
                </Button>
              </div>
            ) : diffLoading ? (
              <EditorSkeleton />
            ) : (
              !pending && <p className='text-xs text-muted-foreground'>Diff no longer available.</p>
            )}
          </CollapsibleContent>
        </Collapsible>
      )}
      {preview.kind === 'definition' && (
        <pre className='max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted p-2 font-mono text-xs'>
          {preview.text}
        </pre>
      )}
      {preview.kind === 'line' && (
        <p className='whitespace-pre-wrap break-all font-mono text-xs'>{preview.text}</p>
      )}

      {proposal.status === 'stale' && (
        <p className='text-xs text-muted-foreground'>
          This item changed after the proposal was made, so nothing was written. Ask the assistant
          to propose it again.
        </p>
      )}
      {proposal.status === 'stale' && failure && (
        <p className='text-xs text-muted-foreground'>{failure}</p>
      )}
      {proposal.status === 'failed' && (
        <p className='text-xs text-destructive'>
          {failure ? `Could not apply this change: ${failure}` : 'Could not apply this change.'}
        </p>
      )}
      {actionError && <p className='text-xs text-destructive'>{actionError}</p>}
      {warning && <p className='text-xs text-warning'>{warning}</p>}

      {pending && (
        <>
          {blockedByEdits && (
            <p className='text-xs text-muted-foreground'>
              This request has unsaved edits in an open tab. Save or discard them before accepting.
            </p>
          )}
          <div className='flex justify-end gap-2'>
            <Button
              size='sm'
              variant='outline'
              disabled={busy}
              onClick={() => void run(rejectProposal)}
            >
              <X className='h-3.5 w-3.5' aria-hidden='true' />
              Reject
            </Button>
            <Button
              size='sm'
              disabled={busy || blockedByEdits}
              onClick={() => void run(acceptProposal)}
            >
              {busy ? (
                <Loader2 className='h-3.5 w-3.5 animate-spin' aria-hidden='true' />
              ) : (
                <Check className='h-3.5 w-3.5' aria-hidden='true' />
              )}
              Accept
            </Button>
          </div>
        </>
      )}
    </article>
  );
}
