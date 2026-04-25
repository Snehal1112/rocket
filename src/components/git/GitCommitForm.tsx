import { Check, Loader2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { useGitStore } from '@/stores/git-store';

export function GitCommitForm() {
  const [message, setMessage] = useState('');
  const [committing, setCommitting] = useState(false);
  const { status, commitChanges } = useGitStore();

  const stagedCount = status?.files.filter((f) => f.staged).length ?? 0;

  const handleCommit = async () => {
    if (!message.trim() || stagedCount === 0) return;
    setCommitting(true);
    try {
      await commitChanges(message.trim());
      setMessage('');
    } finally {
      setCommitting(false);
    }
  };

  return (
    <div className='space-y-2'>
      <Textarea
        placeholder='Commit message... (Ctrl+Enter to commit)'
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) void handleCommit();
        }}
        className='text-sm min-h-[60px] resize-none'
        disabled={committing}
        aria-label='Commit message'
      />
      {stagedCount === 0 && message.trim().length > 0 && (
        <p className='text-xs text-muted-foreground/70'>No files staged</p>
      )}
      <Button
        onClick={handleCommit}
        disabled={!message.trim() || stagedCount === 0 || committing}
        className='w-full'
        size='sm'
      >
        {committing ? (
          <Loader2 className='h-3.5 w-3.5 animate-spin' />
        ) : (
          <Check className='h-3.5 w-3.5' />
        )}
        {committing
          ? 'Committing...'
          : `Commit${stagedCount > 0 ? ` ${stagedCount} file${stagedCount !== 1 ? 's' : ''}` : ''}`}
      </Button>
    </div>
  );
}
