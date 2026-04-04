import { Check } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { useGitStore } from '@/stores/git-store';

export function GitCommitForm() {
  const [message, setMessage] = useState('');
  const { status, commitChanges } = useGitStore();

  const stagedCount = status?.files.filter((f) => f.staged).length ?? 0;

  const handleCommit = async () => {
    if (!message.trim() || stagedCount === 0) return;
    await commitChanges(message.trim());
    setMessage('');
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
      />
      <Button
        onClick={handleCommit}
        disabled={!message.trim() || stagedCount === 0}
        className='w-full'
        size='sm'
      >
        <Check className='h-3.5 w-3.5' />
        Commit Changes
      </Button>
    </div>
  );
}
