import { AlertCircle, Check, GitBranch, GitMerge, Loader2, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button, buttonVariants } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';

export function BranchSelector() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [newBranchName, setNewBranchName] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [switchError, setSwitchError] = useState<string | null>(null);
  const [checkingOutRemote, setCheckingOutRemote] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [switchingTo, setSwitchingTo] = useState<string | null>(null);
  const [mergingName, setMergingName] = useState<string | null>(null);
  const [deletingName, setDeletingName] = useState<string | null>(null);
  // Full remote branch name (e.g. "collections/main") awaiting confirmation
  // to force-reset a colliding local branch onto it.
  const [pendingForceCheckout, setPendingForceCheckout] = useState<string | null>(null);
  // Name typed into the "Checkout as New" field on the collision dialog.
  const [checkoutAsName, setCheckoutAsName] = useState('');
  const branches = useGitStore((state) => state.branches);
  const switchBranch = useGitStore((state) => state.switchBranch);
  const createBranch = useGitStore((state) => state.createBranch);
  const deleteBranch = useGitStore((state) => state.deleteBranch);
  const mergeBranch = useGitStore((state) => state.mergeBranch);
  const checkoutRemoteBranch = useGitStore((state) => state.checkoutRemoteBranch);
  const status = useGitStore((state) => state.status);
  const clearError = useGitStore((state) => state.clearError);
  const gitStoreApi = useGitStoreApi();

  if (!branches) return null;

  const filtered = branches.local.filter((b) =>
    b.name.toLowerCase().includes(search.toLowerCase()),
  );

  const filteredRemote = branches.remote.filter((b) => {
    if (b.name.endsWith('/HEAD')) return false;
    // Hide a remote branch only when a local branch already tracks it
    // specifically — matching on short name alone would also hide a
    // same-named branch on a *different* remote (e.g. local "main" tracking
    // origin/main must not hide collections/main from the list; that other
    // remote branch is still a legitimate, distinct checkout target).
    return (
      !branches.local.some((l) => l.upstream === b.name) &&
      b.name.toLowerCase().includes(search.toLowerCase())
    );
  });

  const handleCreate = async () => {
    if (!newBranchName.trim() || creating) return;
    setCreating(true);
    setCreateError(null);
    clearError();
    try {
      await createBranch(newBranchName.trim());
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        setCreateError(nextError);
      } else {
        setNewBranchName('');
      }
    } finally {
      setCreating(false);
    }
  };

  const handleSwitch = async (name: string) => {
    if (switchingTo) return;
    setSwitchingTo(name);
    setSwitchError(null);
    clearError();
    try {
      await switchBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        setSwitchError(nextError);
      } else {
        setOpen(false);
      }
    } finally {
      setSwitchingTo(null);
    }
  };

  const handleCheckoutRemote = async (name: string, force = false, asName?: string) => {
    setSwitchError(null);
    // Clear any stale error before measuring — otherwise a second, identical
    // collision (e.g. cancel the force dialog, then click the same remote
    // branch again) would compare the new rejection against the leftover
    // error from the first attempt, see no change, and silently report
    // success instead of re-showing the confirmation dialog.
    clearError();
    setCheckingOutRemote(name);
    try {
      await checkoutRemoteBranch(name, force, asName);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        // A non-forced collision with an existing local branch is not a
        // terminal error — offer to force-reset that branch onto this
        // remote branch's content instead of just showing the error.
        if (!force && nextError.includes('already exists')) {
          setPendingForceCheckout(name);
          setCheckoutAsName(name);
        } else {
          setSwitchError(nextError);
        }
      } else {
        setOpen(false);
      }
    } finally {
      setCheckingOutRemote(null);
    }
  };

  const handleConfirmForceCheckout = () => {
    const name = pendingForceCheckout;
    setPendingForceCheckout(null);
    if (name) void handleCheckoutRemote(name, true);
  };

  // Check out the remote branch under a brand-new local branch name instead
  // of resetting the branch that collided, leaving that branch untouched.
  const handleCheckoutAsNew = () => {
    const remoteName = pendingForceCheckout;
    const targetName = checkoutAsName.trim();
    setPendingForceCheckout(null);
    if (remoteName && targetName) void handleCheckoutRemote(remoteName, false, targetName);
  };

  // Short local-branch name that collided (e.g. "main" from "collections/main").
  const pendingLocalName = pendingForceCheckout
    ? pendingForceCheckout.split('/').slice(1).join('/')
    : null;
  // If the colliding branch isn't the one currently checked out, resetting
  // it won't switch you onto it — say so explicitly, since the row you
  // clicked otherwise implies "switch to this."
  const pendingIsCurrent = pendingLocalName !== null && pendingLocalName === status?.branch;

  // Await merge, then surface the result:
  // - On success: close the popover.
  // - On conflict: close the popover so the conflict resolver is visible.
  // - On other error: keep the popover open and show the error inline.
  const handleMerge = async (name: string) => {
    if (mergingName) return;
    setMergingName(name);
    setSwitchError(null);
    clearError();
    try {
      await mergeBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        if (nextError.toLowerCase().includes('conflict')) {
          setOpen(false);
        } else {
          setSwitchError(nextError);
        }
      } else {
        setOpen(false);
      }
    } finally {
      setMergingName(null);
    }
  };

  const handleDelete = async (name: string) => {
    if (deletingName) return;
    setDeletingName(name);
    setSwitchError(null);
    clearError();
    try {
      await deleteBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        setSwitchError(nextError);
      }
    } finally {
      setDeletingName(null);
    }
  };

  return (
    <>
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
        <PopoverContent
          className='w-64 p-0'
          align='start'
          onInteractOutside={(e) => {
            // The force-checkout confirmation dialog renders in its own
            // portal, so Radix sees it as "outside" this popover — without
            // this guard, opening it (or clicking its buttons) would
            // auto-dismiss the popover and hide the confirmation/result.
            if (pendingForceCheckout !== null) e.preventDefault();
          }}
        >
          {switchError && (
            <div
              role='alert'
              className='flex items-start gap-1.5 px-2 py-1.5 text-xs text-destructive border-b border-border/70'
            >
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
                  if (switchingTo) return;
                  if (!branch.isHead) void handleSwitch(branch.name);
                  else setOpen(false);
                }}
                onKeyDown={(e) => {
                  if ((e.key === 'Enter' || e.key === ' ') && !switchingTo) {
                    e.preventDefault();
                    if (!branch.isHead) void handleSwitch(branch.name);
                    else setOpen(false);
                  }
                }}
              >
                {branch.isHead && <Check className='h-3.5 w-3.5 text-primary' />}
                {!branch.isHead && <span className='w-3' />}
                <span className='truncate flex-1'>{branch.name}</span>
                {/* Show which remote this local branch tracks — with more than
                  one remote configured, knowing whether a branch tracks
                  origin or another remote (e.g. collections) is not obvious
                  from the branch name alone. */}
                {branch.upstream && (
                  <span className='shrink-0 text-xs text-muted-foreground/60'>
                    {branch.upstream.split('/')[0]}
                  </span>
                )}
                {!branch.isHead && (
                  <TooltipProvider delayDuration={300}>
                    <div className='branch-row-actions gap-0.5'>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button
                            variant='ghost'
                            size='icon'
                            className='h-5 w-5'
                            aria-label='Merge into current'
                            disabled={mergingName !== null || deletingName !== null}
                            onClick={(e) => {
                              e.stopPropagation();
                              void handleMerge(branch.name);
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
                            aria-label='Delete branch'
                            disabled={mergingName !== null || deletingName !== null}
                            onClick={(e) => {
                              e.stopPropagation();
                              void handleDelete(branch.name);
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
                <div className='px-2 py-1 text-xs text-muted-foreground font-medium mt-1'>
                  Remote
                </div>
                {filteredRemote.map((branch) => {
                  const [remoteName, ...rest] = branch.name.split('/');
                  const localName = rest.join('/');
                  const isCheckingOutThis = checkingOutRemote === branch.name;
                  return (
                    <Button
                      key={branch.name}
                      type='button'
                      variant='ghost'
                      disabled={checkingOutRemote !== null}
                      className='flex w-full h-auto items-center gap-1.5 rounded px-2 py-1 justify-start font-normal hover:bg-muted/50 text-sm text-left disabled:opacity-50 disabled:cursor-not-allowed'
                      onClick={() => {
                        void handleCheckoutRemote(branch.name);
                      }}
                    >
                      {isCheckingOutThis ? (
                        <Loader2 className='w-3.5 h-3.5 animate-spin shrink-0' />
                      ) : (
                        <span className='w-3.5' />
                      )}
                      {/* Show which remote this branch belongs to — with more than
                        one remote configured, two remotes can share a branch
                        name (e.g. origin/main and collections/main), and
                        hiding the remote name made those rows indistinguishable. */}
                      <span className='truncate flex-1 text-muted-foreground'>
                        <span className='text-muted-foreground/60'>{remoteName}/</span>
                        {localName}
                      </span>
                    </Button>
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
                disabled={!newBranchName.trim() || creating}
                aria-label='Create branch'
              >
                <Plus className='h-3.5 w-3.5' />
              </Button>
            </div>
            {createError && (
              <div role='alert' className='flex items-start gap-1.5 text-xs text-destructive'>
                <AlertCircle className='h-3 w-3 shrink-0 mt-0.5' />
                <span className='wrap-break-word'>{createError}</span>
              </div>
            )}
          </div>
        </PopoverContent>
      </Popover>
      <AlertDialog
        open={pendingForceCheckout !== null}
        onOpenChange={(v) => {
          if (!v) {
            setPendingForceCheckout(null);
            setCheckoutAsName('');
          }
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Branch already exists</AlertDialogTitle>
            <AlertDialogDescription>
              A local branch named <span className='font-mono'>{pendingLocalName}</span> already
              exists. Forcing this checkout will reset that local branch to fully match{' '}
              <span className='font-mono'>{pendingForceCheckout}</span>'s content, discarding any
              local commits not present on that remote. This cannot be undone from the UI — only via
              git reflog.
              {!pendingIsCurrent && (
                <>
                  {' '}
                  You're not currently on <span className='font-mono'>{pendingLocalName}</span> —
                  this updates that branch without switching you onto it.
                </>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className='pb-2 space-y-1.5'>
            <p className='text-xs text-muted-foreground'>
              Or check out as a new branch, leaving{' '}
              <span className='font-mono'>{pendingLocalName}</span> untouched:
            </p>
            <div className='flex gap-2'>
              <Input
                value={checkoutAsName}
                onChange={(e) => setCheckoutAsName(e.target.value)}
                className='h-8 text-sm'
                aria-label='New local branch name'
                disabled={checkingOutRemote !== null}
              />
              <Button
                type='button'
                variant='secondary'
                size='sm'
                className='shrink-0'
                disabled={
                  checkingOutRemote !== null ||
                  !checkoutAsName.trim() ||
                  checkoutAsName.trim() === pendingLocalName
                }
                onClick={handleCheckoutAsNew}
              >
                Checkout as New
              </Button>
            </div>
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={checkingOutRemote !== null}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className={cn(buttonVariants({ variant: 'destructive' }))}
              onClick={handleConfirmForceCheckout}
              disabled={checkingOutRemote !== null}
            >
              Reset Branch
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
