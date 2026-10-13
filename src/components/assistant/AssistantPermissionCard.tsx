import { Check, Loader2, ShieldQuestion, X } from 'lucide-react';
import { useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { respondAgentPermission } from '@/lib/tauri-api';
import type { AgentPermission } from '@/stores/assistant-store';

const GENERIC_ACTION_ERROR = 'The answer could not be sent. Try again.';

/** The tool kinds that can change something outside the assistant. */
const CAUTION_LABEL: Record<string, string> = {
  execute: 'Runs a command',
  edit: 'Edits a file',
  delete: 'Deletes a file',
  move: 'Moves a file',
};

/** Only a known allow kind gets the primary look; anything else is neutral. */
function isAllow(kind: string): boolean {
  return kind === 'allow_once' || kind === 'allow_always';
}

/**
 * One permission request from the agent, with a button for each option the
 * agent offered. Once answered or cancelled it is a one-line row. The title
 * and summary are agent text and are always shown as plain text.
 */
export function AssistantPermissionCard({
  sessionId,
  permission,
}: {
  sessionId: string;
  permission: AgentPermission;
}) {
  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  if (permission.status !== 'pending') {
    const answered = permission.status === 'answered';
    const Icon = answered ? Check : X;
    return (
      <div className='flex items-start gap-1.5 rounded-md border bg-muted/30 px-2 py-1.5 text-xs text-muted-foreground'>
        <Icon className='mt-0.5 h-3.5 w-3.5 shrink-0' aria-hidden='true' />
        <span className='min-w-0 break-words'>
          <span className='font-medium'>{permission.title}</span>
          {' - '}
          {answered ? (permission.chosenName ?? 'Answered') : 'Cancelled'}
        </span>
      </div>
    );
  }

  const caution = CAUTION_LABEL[permission.kind];
  const busy = busyId !== null;
  const choose = async (optionId: string) => {
    setBusyId(optionId);
    setActionError(null);
    try {
      await respondAgentPermission(sessionId, permission.requestId, optionId);
      // The store flips the status when the resolved event arrives.
    } catch (err) {
      console.error('[assistant] permission answer failed', err);
      setActionError(GENERIC_ACTION_ERROR);
      setBusyId(null);
    }
  };

  return (
    <article
      aria-label={`Permission request: ${permission.title}`}
      className='flex flex-col gap-2 rounded-md border bg-card p-3'
    >
      <div className='flex items-start justify-between gap-2'>
        <div className='flex min-w-0 items-start gap-2'>
          <ShieldQuestion
            className='mt-0.5 h-4 w-4 shrink-0 text-muted-foreground'
            aria-hidden='true'
          />
          <p className='min-w-0 whitespace-pre-wrap break-words text-sm font-medium'>
            {permission.title}
          </p>
        </div>
        {caution && <Badge variant='warning'>{caution}</Badge>}
      </div>
      {permission.summary && (
        <pre className='max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted p-2 font-mono text-xs'>
          {permission.summary}
        </pre>
      )}
      <p className='text-xs text-muted-foreground'>The agent is waiting for your answer.</p>
      <div className='flex flex-wrap justify-end gap-2'>
        {permission.options.map((option) => (
          <Button
            key={option.id}
            size='sm'
            variant={isAllow(option.kind) ? 'default' : 'outline'}
            className='h-auto min-h-8 max-w-full whitespace-normal break-words py-1.5 text-left'
            disabled={busy}
            onClick={() => void choose(option.id)}
          >
            {busyId === option.id && (
              <Loader2 className='h-3.5 w-3.5 shrink-0 animate-spin' aria-hidden='true' />
            )}
            {option.name}
          </Button>
        ))}
      </div>
      {actionError && (
        <p role='alert' className='text-xs text-destructive'>
          {actionError}
        </p>
      )}
    </article>
  );
}
