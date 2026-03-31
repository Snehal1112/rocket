import { useState } from 'react';
import { Check } from 'lucide-react';
import { Input } from '@/components/ui/input';
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
      <Input
        placeholder="Commit message..."
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        onKeyDown={(e) => { if (e.key === 'Enter') void handleCommit(); }}
        className="text-sm h-8"
      />
      <Button
        onClick={handleCommit}
        disabled={!message.trim() || stagedCount === 0}
        className="w-full"
        size="sm"
      >
        <Check className="h-3.5 w-3.5" />
        Commit Changes
      </Button>
    </div>
  );
}
