import { Check, Loader2, X } from 'lucide-react';
import { lazy, Suspense, useEffect, useState } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  acceptProposal,
  hasDirtyAffectedTab,
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

/** One proposed change with its preview and Accept and Reject. */
export function AssistantProposalCard({ proposal }: { proposal: AgentProposal }) {
  const target = proposalTarget(proposal.change);
  const preview = proposalPreview(proposal.change);
  const blockedByEdits = usePaneStore(
    (s) => proposal.status === 'pending' && hasDirtyAffectedTab(s, proposal.change),
  );
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [diff, setDiff] = useState<ProposalDiff | null>(null);
  const [diffError, setDiffError] = useState<string | null>(null);

  // Loads the diff while the proposal is pending. After Accept the stored
  // version already holds the change, so the earlier diff is kept.
  useEffect(() => {
    if (preview.kind !== 'diff' || proposal.status !== 'pending') return;
    let cancelled = false;
    loadProposalDiff(proposal)
      .then((loaded) => {
        if (!cancelled) setDiff(loaded);
      })
      .catch((err) => {
        if (!cancelled) setDiffError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [proposal, preview.kind]);

  const run = async (action: (p: AgentProposal) => Promise<void>) => {
    setBusy(true);
    setActionError(null);
    try {
      await action(proposal);
    } catch (err) {
      setActionError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const badge = STATUS_BADGE[proposal.status];
  const failure = proposalFailure(proposal);

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

      {preview.kind === 'diff' &&
        (diffError ? (
          <p className='text-xs text-destructive'>
            Could not load the current version: {diffError}
          </p>
        ) : diff ? (
          <Suspense fallback={<EditorSkeleton />}>
            <ProposalDiffEditor
              original={diff.before}
              modified={diff.after}
              language={diff.language}
            />
          </Suspense>
        ) : (
          <EditorSkeleton />
        ))}
      {preview.kind === 'definition' && (
        <MarkdownRenderer restricted>{`\`\`\`json\n${preview.text}\n\`\`\``}</MarkdownRenderer>
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

      {proposal.status === 'pending' && (
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
