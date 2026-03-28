import { useState } from 'react';
import { Textarea } from '@/components/ui/textarea';
import { Button } from '@/components/ui/button';
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
    <div className="space-y-2">
      <Textarea
        placeholder="Commit message..."
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        className="min-h-[60px] resize-y text-sm"
      />
      <Button
        onClick={handleCommit}
        disabled={!message.trim() || stagedCount === 0}
        className="w-full"
        size="sm"
      >
        Commit ({stagedCount} {stagedCount === 1 ? 'file' : 'files'})
      </Button>
    </div>
  );
}
