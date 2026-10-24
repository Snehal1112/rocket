import { Check, ChevronDown, ChevronRight, Loader2, X } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import type { ToolMessage } from '@/lib/assistant/timeline';
import { cn } from '@/lib/utils';
import type { ToolActivityStatus } from '@/stores/assistant-store';

const TOOL_STATUS_LABEL: Record<ToolActivityStatus, string> = {
  pending: 'waiting',
  in_progress: 'running',
  completed: 'done',
  failed: 'failed',
};

const isBusy = (status: ToolActivityStatus) => status === 'pending' || status === 'in_progress';

/** The function name of a tool title: the text before the first `(` or `:`. */
export function toolName(title: string): string {
  const end = title.search(/[(:]/);
  return (end === -1 ? title : title.slice(0, end)).trim() || title;
}

function ToolRow({ title, status }: { title: string; status: ToolActivityStatus }) {
  const Icon = status === 'completed' ? Check : status === 'failed' ? X : Loader2;
  return (
    <li className='flex items-center gap-1.5 font-mono text-xs text-muted-foreground'>
      <Icon
        className={cn(
          'h-3 w-3 shrink-0',
          isBusy(status) && 'animate-spin',
          status === 'failed' && 'text-destructive',
        )}
        aria-hidden='true'
      />
      <span className='truncate' title={title}>
        {title}
      </span>
      <span className='sr-only'>{TOOL_STATUS_LABEL[status]}</span>
    </li>
  );
}

/** A run of tool calls as one collapsed summary row. Never opens on its own. */
export function ToolRun({ tools }: { tools: ToolMessage[] }) {
  const [open, setOpen] = useState(false);
  const Chevron = open ? ChevronDown : ChevronRight;
  const running = tools.filter((t) => isBusy(t.status));
  const failed = tools.filter((t) => t.status === 'failed').length;
  const latest = running[running.length - 1];
  const summary = latest
    ? `Working: ${toolName(latest.title)}`
    : `Used ${tools.length} ${tools.length === 1 ? 'tool' : 'tools'}`;
  return (
    <Collapsible open={open} onOpenChange={setOpen} className='min-w-0'>
      <CollapsibleTrigger asChild>
        <Button
          variant='ghost'
          size='sm'
          className='-ml-1.5 h-7 max-w-full justify-start gap-1.5 px-1.5 text-xs font-normal text-muted-foreground'
        >
          <Chevron className='h-3.5 w-3.5 shrink-0' aria-hidden='true' />
          {latest && <Loader2 className='h-3 w-3 shrink-0 animate-spin' aria-hidden='true' />}
          <span className='truncate'>{summary}</span>
          {failed > 0 && <span className='shrink-0 text-destructive'>{` · ${failed} failed`}</span>}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className='flex flex-col gap-1 py-1 pl-6'>
          {tools.map((t) => (
            <ToolRow key={t.id} title={t.title} status={t.status} />
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  );
}
