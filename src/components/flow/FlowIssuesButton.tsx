import { CircleAlert, TriangleAlert } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { type FlowIssue, issueCountLabel, worstSeverity } from '@/lib/flow-issues';
import type { FlowNode } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface FlowIssuesButtonProps {
  issues: FlowIssue[];
  nodes: FlowNode[];
  onSelectNode: (nodeId: string) => void;
}

function SeverityIcon({ severity }: { severity: FlowIssue['severity'] }) {
  const Icon = severity === 'error' ? CircleAlert : TriangleAlert;
  return (
    <Icon
      className={cn(
        'mt-0.5 h-3.5 w-3.5 shrink-0',
        severity === 'error' ? 'text-red-500' : 'text-amber-500',
      )}
      aria-hidden='true'
    />
  );
}

// A count next to Run. The popover lists every issue, and a click on one that
// names a node selects it and opens its panel.
export function FlowIssuesButton({ issues, nodes, onSelectNode }: FlowIssuesButtonProps) {
  const [open, setOpen] = useState(false);
  if (issues.length === 0) return null;
  const worst = worstSeverity(issues) ?? 'warning';
  const nodeLabel = (nodeId: string) => {
    const node = nodes.find((n) => n.id === nodeId);
    return node ? node.kind.label.trim() || node.id : nodeId;
  };
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='gap-1.5'
          aria-label={issueCountLabel(issues)}
        >
          <SeverityIcon severity={worst} />
          <span>{issues.length}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent align='end' className='nokey w-80 p-1'>
        <ul aria-label='Flow issues' className='max-h-80 overflow-y-auto'>
          {issues.map((issue) => {
            const nodeId = issue.nodeId;
            const key = `${issue.code}:${nodeId ?? issue.edgeId ?? ''}:${issue.message}`;
            return (
              <li key={key}>
                {nodeId ? (
                  <Button
                    type='button'
                    variant='ghost'
                    size='sm'
                    className='h-auto w-full items-start justify-start gap-2 whitespace-normal px-2 py-1.5 text-left text-xs'
                    onClick={() => {
                      onSelectNode(nodeId);
                      setOpen(false);
                    }}
                  >
                    <SeverityIcon severity={issue.severity} />
                    <span>
                      <span className='font-medium'>{nodeLabel(nodeId)}</span>
                      {': '}
                      {issue.message}
                    </span>
                  </Button>
                ) : (
                  <div className='flex items-start gap-2 px-2 py-1.5 text-xs'>
                    <SeverityIcon severity={issue.severity} />
                    <span>
                      <span className='font-medium'>Wire</span>
                      {': '}
                      {issue.message}
                    </span>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      </PopoverContent>
    </Popover>
  );
}
