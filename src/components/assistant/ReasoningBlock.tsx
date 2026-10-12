import { Brain, ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/** The agent's reasoning for the current turn. Collapsed until the user opens it. */
export function ReasoningBlock() {
  const reasoning = useAssistantStore((s) => s.reasoning);
  const running = useAssistantStore(selectTurnRunning);
  const [open, setOpen] = useState(false);
  if (!reasoning) return null;
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <Collapsible open={open} onOpenChange={setOpen} className='rounded-md border bg-muted/30'>
      <CollapsibleTrigger asChild>
        <Button variant='ghost' size='sm' className='h-7 w-full justify-start gap-1 px-2 text-xs'>
          <Chevron className='h-3 w-3' />
          <Brain className='h-3 w-3' />
          {running ? 'Reasoning (live)' : 'Reasoning'}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <p className='max-h-48 overflow-auto whitespace-pre-wrap px-2 pb-2 text-xs text-muted-foreground'>
          {reasoning}
        </p>
      </CollapsibleContent>
    </Collapsible>
  );
}
