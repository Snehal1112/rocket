import { ChevronDown } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';

export interface CollapsibleSectionProps {
  title: string;
  summary?: string;
  /** Initial state when uncontrolled. */
  defaultOpen?: boolean;
  /** When given, the section is controlled and `defaultOpen` is ignored. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: ReactNode;
}

/**
 * A collapsible group. The header row shows the title and a one-line summary.
 * A parent can open it, for example to show an error, through `open` and `onOpenChange`.
 */
export function CollapsibleSection({
  title,
  summary,
  defaultOpen = false,
  open,
  onOpenChange,
  children,
}: CollapsibleSectionProps) {
  const stateProps = open === undefined ? { defaultOpen, onOpenChange } : { open, onOpenChange };
  return (
    <Collapsible {...stateProps} className='rounded-md border'>
      <CollapsibleTrigger asChild>
        <Button
          variant='ghost'
          className='group h-auto w-full justify-start gap-2 rounded-b-none px-3 py-2 text-left'
        >
          <ChevronDown
            aria-hidden='true'
            className='h-4 w-4 shrink-0 text-muted-foreground transition-transform motion-reduce:transition-none group-data-[state=open]:rotate-180'
          />
          <span className='text-sm font-medium'>{title}</span>
          {summary && (
            <span className='min-w-0 flex-1 truncate text-xs font-normal text-muted-foreground'>
              {summary}
            </span>
          )}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className='space-y-4 border-t px-3 py-3'>{children}</CollapsibleContent>
    </Collapsible>
  );
}
