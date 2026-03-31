import { useState } from 'react';
import { Check, GitBranch, Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Separator } from '@/components/ui/separator';
import { useGitStore } from '@/stores/git-store';

export function BranchSelector() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [newBranchName, setNewBranchName] = useState('');
  const { branches, switchBranch, createBranch, deleteBranch, mergeBranch, checkoutRemoteBranch, status } = useGitStore();

  if (!branches) return null;

  const filtered = branches.local.filter((b) =>
    b.name.toLowerCase().includes(search.toLowerCase())
  );

  const filteredRemote = branches.remote
    .filter((b) => {
      // Exclude HEAD pointer and branches that already have a local counterpart.
      if (b.name.endsWith('/HEAD')) return false;
      const localName = b.name.split('/').slice(1).join('/');
      return (
        !branches.local.some((l) => l.name === localName) &&
        b.name.toLowerCase().includes(search.toLowerCase())
      );
    });

  const handleCreate = async () => {
    if (!newBranchName.trim()) return;
    await createBranch(newBranchName.trim());
    setNewBranchName('');
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" className="h-6 gap-1 text-sm">
          <GitBranch className="h-3.5 w-3.5" />
          {status?.branch ?? 'main'}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-64 p-0" align="start">
        <div className="p-2">
          <Input
            placeholder="Search branches..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="h-7 text-sm"
          />
        </div>
        <Separator />
        <div className="max-h-48 overflow-y-auto p-1">
          {filtered.map((branch) => (
            <div
              key={branch.name}
              className="group flex items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm"
              onClick={() => {
                if (!branch.isHead) switchBranch(branch.name);
                setOpen(false);
              }}
            >
              {branch.isHead && <Check className="h-3.5 w-3.5 text-primary" />}
              {!branch.isHead && <span className="w-3" />}
              <span className="truncate flex-1">{branch.name}</span>
              {!branch.isHead && (
                <div className="flex gap-0.5 opacity-0 group-hover:opacity-100">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-5 w-5"
                    onClick={(e) => {
                      e.stopPropagation();
                      mergeBranch(branch.name);
                      setOpen(false);
                    }}
                    title="Merge into current"
                  >
                    <GitBranch className="h-3.5 w-3.5 text-muted-foreground" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-5 w-5 text-destructive"
                    onClick={(e) => {
                      e.stopPropagation();
                      deleteBranch(branch.name);
                    }}
                    title="Delete branch"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              )}
            </div>
          ))}
          {filteredRemote.length > 0 && (
            <>
              <div className="px-2 py-1 text-xs text-muted-foreground font-medium mt-1">
                Remote
              </div>
              {filteredRemote.map((branch) => {
                const localName = branch.name.split('/').slice(1).join('/');
                return (
                  <div
                    key={branch.name}
                    className="flex items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm"
                    onClick={() => {
                      checkoutRemoteBranch(branch.name);
                      setOpen(false);
                    }}
                  >
                    <span className="w-3.5" />
                    <span className="truncate flex-1 text-muted-foreground">{localName}</span>
                  </div>
                );
              })}
            </>
          )}
        </div>
        <Separator />
        <div className="flex gap-1 p-2">
          <Input
            placeholder="New branch..."
            value={newBranchName}
            onChange={(e) => setNewBranchName(e.target.value)}
            className="h-7 text-sm"
            onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
          />
          <Button
            variant="outline"
            size="sm"
            className="h-7 shrink-0"
            onClick={handleCreate}
            disabled={!newBranchName.trim()}
          >
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
