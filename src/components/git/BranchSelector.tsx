import { AlertCircle, Check, GitBranch, GitMerge, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useGitStore } from '@/stores/git-store';

export function BranchSelector() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [newBranchName, setNewBranchName] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [switchError, setSwitchError] = useState<string | null>(null);
  const {
    branches,
    switchBranch,
    createBranch,
    deleteBranch,
    mergeBranch,
    checkoutRemoteBranch,
    status,
  } = useGitStore();

  if (!branches) return null;

  const filtered = branches.local.filter((b) =>
    b.name.toLowerCase().includes(search.toLowerCase()),
  );

  const filteredRemote = branches.remote.filter((b) => {
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
    setCreateError(null);
    const prevError = useGitStore.getState().error;
    await createBranch(newBranchName.trim());
    const nextError = useGitStore.getState().error;
    // If the store recorded a new error, surface it inline rather than silently dropping it.
    if (nextError && nextError !== prevError) {
      setCreateError(nextError);
    } else {
      setNewBranchName('');
    }
  };

  // Switch to a local branch, keeping the popover open on failure so the
  // error is visible rather than silently dropped into the store.
  const handleSwitch = async (name: string) => {
    setSwitchError(null);
    const prevError = useGitStore.getState().error;
    await switchBranch(name);
    const nextError = useGitStore.getState().error;
    if (nextError && nextError !== prevError) {
      setSwitchError(nextError);
    } else {
      setOpen(false);
    }
  };

  // Check out a remote branch, keeping the popover open on failure.
  const handleCheckoutRemote = async (name: string) => {
    setSwitchError(null);
    const prevError = useGitStore.getState().error;
    await checkoutRemoteBranch(name);
    const nextError = useGitStore.getState().error;
    if (nextError && nextError !== prevError) {
      setSwitchError(nextError);
    } else {
      setOpen(false);
    }
  };

  return (
    <Popover
      open={open}
      onOpenChange={(v) => {
        setOpen(v);
        if (!v) setSwitchError(null);
      }}
    >
      <PopoverTrigger asChild>
        <Button variant='ghost' size='sm' className='h-6 gap-1 text-sm'>
          <GitBranch className='h-3.5 w-3.5' />
          {status?.branch ?? 'main'}
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-64 p-0' align='start'>
        {switchError && (
          <div className='flex items-start gap-1.5 px-2 py-1.5 text-xs text-destructive border-b border-border/70'>
            <AlertCircle className='h-3 w-3 shrink-0 mt-0.5' />
            <span className='wrap-break-word'>{switchError}</span>
          </div>
        )}
        <div className='p-2'>
          <Input
            placeholder='Search branches...'
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setSwitchError(null);
            }}
            className='h-7 text-sm'
            aria-label='Search branches'
          />
        </div>
        <Separator />
        <div className='max-h-48 overflow-y-auto p-1'>
          {filtered.map((branch) => (
            // biome-ignore lint/a11y/useSemanticElements: outer <button> nesting inner <button> is invalid HTML; WebKitGTK reparses it and breaks hover tracking.
            <div
              key={branch.name}
              role='button'
              tabIndex={0}
              className='branch-row flex w-full items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm text-left'
              onClick={() => {
                if (!branch.isHead) void handleSwitch(branch.name);
                else setOpen(false);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  if (!branch.isHead) void handleSwitch(branch.name);
                  else setOpen(false);
                }
              }}
            >
              {branch.isHead && <Check className='h-3.5 w-3.5 text-primary' />}
              {!branch.isHead && <span className='w-3' />}
              <span className='truncate flex-1'>{branch.name}</span>
              {!branch.isHead && (
                <TooltipProvider delayDuration={300}>
                  <div className='branch-row-actions gap-0.5'>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            mergeBranch(branch.name);
                            setOpen(false);
                          }}
                        >
                          <GitMerge className='h-3.5 w-3.5 text-muted-foreground' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Merge into current</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5 text-destructive'
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteBranch(branch.name);
                          }}
                        >
                          <Trash2 className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Delete branch</TooltipContent>
                    </Tooltip>
                  </div>
                </TooltipProvider>
              )}
            </div>
          ))}
          {filteredRemote.length > 0 && (
            <>
              <div className='px-2 py-1 text-xs text-muted-foreground font-medium mt-1'>Remote</div>
              {filteredRemote.map((branch) => {
                const localName = branch.name.split('/').slice(1).join('/');
                return (
                  <button
                    key={branch.name}
                    type='button'
                    className='flex w-full items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm text-left'
                    onClick={() => {
                      void handleCheckoutRemote(branch.name);
                    }}
                  >
                    <span className='w-3.5' />
                    <span className='truncate flex-1 text-muted-foreground'>{localName}</span>
                  </button>
                );
              })}
            </>
          )}
        </div>
        <Separator />
        <div className='flex flex-col gap-1 p-2'>
          <div className='flex gap-1'>
            <Input
              placeholder='New branch...'
              value={newBranchName}
              onChange={(e) => {
                setNewBranchName(e.target.value);
                setCreateError(null);
              }}
              className='h-7 text-sm'
              onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
              aria-label='New branch name'
            />
            <Button
              variant='outline'
              size='sm'
              className='h-7 shrink-0'
              onClick={handleCreate}
              disabled={!newBranchName.trim()}
              aria-label='Create branch'
            >
              <Plus className='h-3.5 w-3.5' />
            </Button>
          </div>
          {createError && (
            <div className='flex items-start gap-1.5 text-xs text-destructive'>
              <AlertCircle className='h-3 w-3 shrink-0 mt-0.5' />
              <span className='wrap-break-word'>{createError}</span>
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
