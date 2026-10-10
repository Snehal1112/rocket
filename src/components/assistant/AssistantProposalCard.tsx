import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronRight,
  Clock,
  Loader2,
  X,
  XCircle,
} from 'lucide-react';
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
import { cn } from '@/lib/utils';
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

const INLINE_STATUS_ICON = {
  pending: Clock,
  accepted: Check,
  rejected: X,
  stale: AlertTriangle,
  failed: XCircle,
} as const;

const GENERIC_ACTION_ERROR = 'The action failed. Try again.';

/**
 * One proposed change with its preview and Accept and Reject. With `inline`
 * it is a collapsed one-line row for a resolved proposal, without actions.
 */
export function AssistantProposalCard({
  proposal,
  inline = false,
}: {
  proposal: AgentProposal;
  inline?: boolean;
}) {
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

  const diffContent = (
    <>
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
    </>
  );

  const textPreview = (
    <>
      {preview.kind === 'definition' && (
        <pre className='max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted p-2 font-mono text-xs'>
          {preview.text}
        </pre>
      )}
      {preview.kind === 'line' && (
        <p className='whitespace-pre-wrap break-all font-mono text-xs'>{preview.text}</p>
      )}
    </>
  );

  const statusNotes = (
    <>
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
    </>
  );

  const targetLine = (
    <p className='truncate text-xs text-muted-foreground'>
      {target.path ? `${target.collection} / ${target.path}` : target.collection}
    </p>
  );

  if (inline) {
    const StatusIcon = INLINE_STATUS_ICON[proposal.status];
    return (
      <Collapsible
        open={expanded}
        onOpenChange={setExpanded}
        role='group'
        aria-label={`${badge.label} change: ${proposal.summary}`}
        className='rounded-md border bg-muted/30'
      >
        <CollapsibleTrigger asChild>
          <Button
            variant='ghost'
            size='sm'
            className='h-7 w-full justify-start gap-1.5 px-2 text-xs font-normal'
          >
            <Chevron className='h-3.5 w-3.5 shrink-0' aria-hidden='true' />
            <StatusIcon
              className={cn(
                'h-3.5 w-3.5 shrink-0',
                proposal.status === 'failed' && 'text-destructive',
                proposal.status === 'stale' && 'text-warning',
              )}
              aria-hidden='true'
            />
            <span className='shrink-0 font-medium'>{badge.label}</span>
            <span className='min-w-0 truncate text-muted-foreground'>{proposal.summary}</span>
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className='flex flex-col gap-2 px-2 pb-2'>
          <p className='text-sm'>{proposal.summary}</p>
          {targetLine}
          {preview.kind === 'diff' && diffContent}
          {textPreview}
          {statusNotes}
        </CollapsibleContent>
      </Collapsible>
    );
  }

  return (
    <article
      aria-label={proposal.summary}
      className='flex flex-col gap-2 rounded-md border bg-card p-3'
    >
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <p className='text-sm font-medium'>{proposal.summary}</p>
          {targetLine}
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
          <CollapsibleContent className='pt-1'>{diffContent}</CollapsibleContent>
        </Collapsible>
      )}
      {textPreview}
      {statusNotes}
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
