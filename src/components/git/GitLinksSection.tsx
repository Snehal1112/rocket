import { Archive, History, Link } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface GitLinksSectionProps {
  onNavigate: (view: 'commits' | 'stashes') => void;
  onOpenRemotes: () => void;
}

export function GitLinksSection({ onNavigate, onOpenRemotes }: GitLinksSectionProps) {
  return (
    <div className='px-3 py-2 space-y-0.5'>
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
  );
}
