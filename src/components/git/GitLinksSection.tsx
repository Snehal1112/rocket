import { Archive, ChevronDown, History, Link } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';

interface GitLinksSectionProps {
  onNavigate: (view: 'commits' | 'stashes') => void;
  onOpenRemotes: () => void;
}

export function GitLinksSection({ onNavigate, onOpenRemotes }: GitLinksSectionProps) {
  const [open, setOpen] = useState(true);

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <div className='flex items-center gap-1 px-3 py-2 cursor-pointer text-sm font-medium text-primary'>
          <ChevronDown
            className={`h-3.5 w-3.5 transition-transform ${!open ? '-rotate-90' : ''}`}
          />
          Links
        </div>
      </CollapsibleTrigger>

      <CollapsibleContent>
        <div className='px-3 pb-2 space-y-0.5'>
          <Button
            variant='ghost'
            size='sm'
            className='w-full justify-start gap-2 h-8 text-sm font-normal'
            onClick={() => onNavigate('commits')}
          >
            <History className='h-3.5 w-3.5 text-muted-foreground' />
            Commits
          </Button>

          <Button
            variant='ghost'
            size='sm'
            className='w-full justify-start gap-2 h-8 text-sm font-normal'
            onClick={() => onNavigate('stashes')}
          >
            <Archive className='h-3.5 w-3.5 text-muted-foreground' />
            Stashes
          </Button>

          <Button
            variant='ghost'
            size='sm'
            className='w-full justify-start gap-2 h-8 text-sm font-normal'
            onClick={onOpenRemotes}
          >
            <Link className='h-3.5 w-3.5 text-muted-foreground' />
            Remotes
          </Button>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
