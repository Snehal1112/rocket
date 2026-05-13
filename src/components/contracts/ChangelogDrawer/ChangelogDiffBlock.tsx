import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { DiffLine } from '@/types/contracts';

interface ChangelogDiffBlockProps {
  diffLines: DiffLine[];
}

const COLLAPSE_THRESHOLD = 6;

export function ChangelogDiffBlock({ diffLines }: ChangelogDiffBlockProps) {
  const [expanded, setExpanded] = useState(false);

  const shouldCollapse = diffLines.length > COLLAPSE_THRESHOLD;
  const visibleLines =
    shouldCollapse && !expanded ? diffLines.slice(0, COLLAPSE_THRESHOLD) : diffLines;

  return (
    <div className='mt-1.5 px-2.5 py-2 bg-background border border-border rounded-sm font-mono text-[11px] w-full'>
      {visibleLines.map((line, idx) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: diff lines have no stable identity
          key={`${idx}-${line.kind}`}
          className={cn(
            'whitespace-pre-wrap break-all',
            line.kind === 'add' && 'text-[hsl(var(--success))]',
            line.kind === 'remove' && 'text-destructive',
            line.kind === 'context' && 'text-muted-foreground',
          )}
        >
          {line.kind === 'add' && '+ '}
          {line.kind === 'remove' && '− '}
          {line.kind === 'context' && '  '}
          {line.text}
        </div>
      ))}
      {shouldCollapse && (
        <Button
          variant='link'
          size='sm'
          className='text-[11px] h-auto p-0 mt-1 font-mono'
          onClick={() => setExpanded((e) => !e)}
        >
          {expanded
            ? 'Collapse diff'
            : `Show full diff (${diffLines.length - COLLAPSE_THRESHOLD} more lines)`}
        </Button>
      )}
    </div>
  );
}
