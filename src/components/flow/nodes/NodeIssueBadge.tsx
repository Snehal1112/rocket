import { CircleAlert, TriangleAlert } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { type FlowIssue, summarizeIssues, worstSeverity } from '@/lib/flow-issues';
import { cn } from '@/lib/utils';

// A small icon in a node header. It carries no text, so the node card's text
// stays the same. The accessible name lists every issue, and the tooltip shows
// the same lines on hover.
export function NodeIssueBadge({ issues }: { issues?: FlowIssue[] }) {
  if (!issues || issues.length === 0) return null;
  const severity = worstSeverity(issues);
  const Icon = severity === 'error' ? CircleAlert : TriangleAlert;
  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            role='img'
            aria-label={summarizeIssues(issues)}
            data-testid='node-issue-badge'
            data-severity={severity}
            className={cn(
              'inline-flex shrink-0',
              severity === 'error' ? 'text-red-500' : 'text-amber-500',
            )}
          >
            <Icon className='h-3.5 w-3.5' aria-hidden='true' />
          </span>
        </TooltipTrigger>
        <TooltipContent className='max-w-64 space-y-1'>
          {issues.map((i) => (
            <p key={`${i.code}:${i.message}`}>{i.message}</p>
          ))}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
