import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Archive, MoreHorizontal } from 'lucide-react';
import { useGitStore } from '@/stores/git-store';

export function GitStashSection() {
  const [message, setMessage] = useState('');
  const { stashes, saveStash, popStash, applyStash, dropStash } = useGitStore();

  const handleSave = async () => {
    if (!message.trim()) return;
    await saveStash(message.trim());
    setMessage('');
  };

  return (
    <div>
      <div className="flex items-center gap-1 px-2 py-1">
        <Archive className="h-3.5 w-3.5 text-muted-foreground" />
        <span className="text-xs font-medium text-muted-foreground">
          Stash ({stashes.length})
        </span>
      </div>
      <div className="flex gap-1 px-2 pb-1">
        <Input
          placeholder="Stash message..."
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          className="h-7 text-xs"
          onKeyDown={(e) => e.key === 'Enter' && handleSave()}
        />
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs shrink-0"
          onClick={handleSave}
          disabled={!message.trim()}
        >
          Save
        </Button>
      </div>
      {stashes.map((stash) => (
        <div
          key={stash.index}
          className="group flex items-center gap-1.5 rounded px-2 py-0.5 hover:bg-muted/50 text-sm"
        >
          <span className="truncate text-xs">{stash.message}</span>
          {stash.branch && (
            <span className="text-xs text-muted-foreground">({stash.branch})</span>
          )}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="ml-auto h-5 w-5 opacity-0 group-hover:opacity-100"
              >
                <MoreHorizontal className="h-3 w-3" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => popStash(stash.index)}>Pop</DropdownMenuItem>
              <DropdownMenuItem onClick={() => applyStash(stash.index)}>Apply</DropdownMenuItem>
              <DropdownMenuItem
                className="text-destructive"
                onClick={() => dropStash(stash.index)}
              >
                Drop
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ))}
    </div>
  );
}
