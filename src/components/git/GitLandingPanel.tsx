import {
  AlertCircle,
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  Check,
  ChevronDown,
  Clock,
  GitBranch,
  GitCommit,
  KeyRound,
  Loader2,
  RefreshCw,
} from 'lucide-react';
import { useState } from 'react';
import gitIcon from '@/assets/git-icon.svg';
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
import { Badge } from '@/components/ui/badge';
import { Button, buttonVariants } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { resolveActiveRemote } from '@/stores/git-store';
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';

export function GitLandingPanel() {
  const status = useGitStore((s) => s.status);
  const push = useGitStore((s) => s.push);
  const pull = useGitStore((s) => s.pull);
  const fetch = useGitStore((s) => s.fetch);
  const saveStash = useGitStore((s) => s.saveStash);
  const popStash = useGitStore((s) => s.popStash);
  const error = useGitStore((s) => s.error);
  const clearError = useGitStore((s) => s.clearError);
  const credentials = useGitStore((s) => s.credentials);
  const setShowCredentialsDialog = useGitStore((s) => s.setShowCredentialsDialog);
  const remotes = useGitStore((s) => s.remotes);
  const branches = useGitStore((s) => s.branches);
  const gitStoreApi = useGitStoreApi();
  const activeRemote = resolveActiveRemote({ branches, remotes });

  const [pushing, setPushing] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [lastFetched, setLastFetched] = useState<string | null>(null);
  const [showStashDialog, setShowStashDialog] = useState(false);
  const [showFetchFirstDialog, setShowFetchFirstDialog] = useState(false);
  const [showForcePushDialog, setShowForcePushDialog] = useState(false);

  const handleFetch = async () => {
    const { credentials } = gitStoreApi.getState();
    if (!credentials) {
      // Store will open the credentials dialog; skip timestamp update.
      fetch();
      return;
    }
    setFetching(true);
    try {
      await fetch();
      setLastFetched(new Date().toLocaleTimeString());
    } finally {
      setFetching(false);
    }
  };

  const handlePull = async () => {
    const { credentials } = gitStoreApi.getState();
    if (!credentials) {
      pull();
      return;
    }

    // Check if working tree has uncommitted changes.
    const { status: currentStatus } = gitStoreApi.getState();
    if (currentStatus && !currentStatus.isClean) {
      setShowStashDialog(true);
      return;
    }

    setPulling(true);
    try {
      await pull();
      setLastFetched(new Date().toLocaleTimeString());
    } finally {
      setPulling(false);
    }
  };

  const handleStashAndPull = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await saveStash('Auto-stash before pull');
      await pull();
      // After pull, check whether it produced merge conflicts.
      // If so, do NOT restore the stash — applying it on top of a conflicted
      // index would corrupt the working tree with doubled conflicts.
      if (gitStoreApi.getState().hasConflicts()) {
        // Leave the stash in place; the user can pop it after resolving conflicts.
        return;
      }
      await popStash(0);
      setLastFetched(new Date().toLocaleTimeString());
    } catch {
      // If pop fails (e.g. stash itself conflicts), stash is preserved for manual resolution.
    } finally {
      setPulling(false);
    }
  };

  const handlePullAnyway = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await pull();
      setLastFetched(new Date().toLocaleTimeString());
    } finally {
      setPulling(false);
    }
  };

  const handlePush = async () => {
    const { credentials } = gitStoreApi.getState();
    if (!credentials) {
      push();
      return;
    }

    // Suggest fetching first if never fetched this session or behind remote.
    const { status: currentStatus } = gitStoreApi.getState();
    if (!lastFetched || (currentStatus && currentStatus.behind > 0)) {
      setShowFetchFirstDialog(true);
      return;
    }

    setPushing(true);
    try {
      await push();
    } finally {
      setPushing(false);
    }
  };

  const handleFetchAndPush = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try {
      await fetch();
      setLastFetched(new Date().toLocaleTimeString());
      // Re-check status after fetch — if now behind, abort push.
      const { status: freshStatus } = gitStoreApi.getState();
      if (freshStatus && freshStatus.behind > 0) {
        setPushing(false);
        return;
      }
      await push();
    } finally {
      setPushing(false);
    }
  };

  const handlePushAnyway = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try {
      await push();
    } finally {
      setPushing(false);
    }
  };

  const handleForcePushClick = () => {
    const { credentials } = gitStoreApi.getState();
    if (!credentials) {
      // Resolve credentials first, exactly like handlePush does — the
      // generic credential-resume path (activatePendingCredentials) replays
      // a bare `push()` with no arguments, which would silently drop the
      // force flag and turn a confirmed force-push into a normal one. Never
      // show the force-push confirmation until credentials already exist,
      // so the resume path is never involved in a forced push.
      push();
      return;
    }
    setShowForcePushDialog(true);
  };

  const handleForcePush = async () => {
    setShowForcePushDialog(false);
    setPushing(true);
    try {
      await push(undefined, true);
    } finally {
      setPushing(false);
    }
  };

  const ahead = status?.ahead ?? 0;
  const behind = status?.behind ?? 0;
  const isUpToDate = (status?.isClean ?? false) && ahead === 0 && behind === 0;
  const hasConflicts = useGitStore((state) => state.hasConflicts?.()) ?? false;

  return (
    <div className='flex flex-col items-center justify-center h-full px-6'>
      <Card className='w-full max-w-md border-border/40 rounded-md bg-card/80 '>
        {/* Header: branch name + ahead/behind sync counts */}
        <CardHeader className='px-4 py-3 space-y-1 border-b border-border/70'>
          <div className='flex items-center gap-2'>
            <div className='flex items-center gap-1.5 min-w-0 flex-1'>
              <GitBranch className='h-4 w-4 text-muted-foreground shrink-0' />
              <span className='font-mono text-sm font-semibold truncate'>
                {status?.branch ?? 'no branch'}
              </span>
            </div>
            <div className='flex items-center gap-1.5 shrink-0'>
              <Badge
                variant='secondary'
                className={cn(
                  'text-xs tabular-nums',
                  ahead > 0 ? 'text-amber-500 bg-amber-500/10' : 'text-muted-foreground',
                )}
              >
                ↑{ahead}
              </Badge>
              <Badge
                variant='secondary'
                className={cn(
                  'text-xs tabular-nums',
                  behind > 0 ? 'text-amber-500 bg-amber-500/10' : 'text-muted-foreground',
                )}
              >
                ↓{behind}
              </Badge>
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant='ghost'
                      size='icon'
                      className={cn(
                        'h-6 w-6',
                        credentials ? 'text-muted-foreground' : 'text-amber-500',
                      )}
                      onClick={() => setShowCredentialsDialog(true)}
                      aria-label='Change SSH credentials'
                    >
                      <KeyRound className='h-3.5 w-3.5' />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {credentials ? 'Change credentials' : 'Set credentials'}
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            </div>
          </div>
        </CardHeader>

        <CardContent className='px-4 py-4 space-y-3'>
          {/* Hero icon + hint text */}
          <div className='flex flex-col items-center justify-center gap-2 py-3'>
            <img src={gitIcon} alt='Git' className='h-12 w-12 opacity-80' />
            <p className='text-xs text-muted-foreground/60 items-center text-center leading-relaxed max-w-50'>
              Perform git actions or open files from sidebar to view
            </p>
          </div>

          {/* Fetch / Pull / Push actions — grid-cols-3 so all three columns stay
            equal width; flex-1 on a nested flex child (the Push+chevron group)
            gets squeezed narrower than its siblings because the chevron button's
            own min-content width eats into that column's fair share. */}
          <div className='grid grid-cols-3 gap-2'>
            <Button
              variant='outline'
              size='sm'
              className='w-full'
              onClick={handleFetch}
              disabled={fetching}
            >
              {fetching ? (
                <Loader2 className='h-3.5 w-3.5 animate-spin' />
              ) : (
                <RefreshCw className='h-3.5 w-3.5' />
              )}
              Fetch
            </Button>
            <Button
              variant='outline'
              size='sm'
              className='w-full'
              onClick={handlePull}
              disabled={pulling}
            >
              {pulling ? (
                <Loader2 className='h-3.5 w-3.5 animate-spin' />
              ) : (
                <ArrowDown className='h-3.5 w-3.5' />
              )}
              Pull{behind > 0 ? ` ↓${behind}` : ''}
            </Button>
            <div className='flex w-full'>
              <Button
                variant={ahead > 0 ? 'default' : 'outline'}
                size='sm'
                className='flex-1 rounded-r-none border-r-0'
                onClick={handlePush}
                disabled={pushing || hasConflicts}
              >
                {pushing ? (
                  <Loader2 className='h-3.5 w-3.5 animate-spin' />
                ) : (
                  <ArrowUp className='h-3.5 w-3.5' />
                )}
                Push{ahead > 0 ? ` ↑${ahead}` : ''}
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant={ahead > 0 ? 'default' : 'outline'}
                    size='sm'
                    className='rounded-l-none px-2'
                    disabled={pushing || hasConflicts}
                    aria-label='More push options'
                  >
                    <ChevronDown className='h-3.5 w-3.5' />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align='end'>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    className='text-destructive focus:text-destructive'
                    onClick={handleForcePushClick}
                    disabled={pushing || hasConflicts}
                  >
                    <AlertTriangle className='h-3.5 w-3.5 mr-2 shrink-0 text-destructive' /> Force
                    Push
                    {hasConflicts && (
                      <span className='ml-auto pl-2 text-[10px] text-muted-foreground'>
                        resolve conflicts first
                      </span>
                    )}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>

          {/* Inline error alert for failed push/pull/fetch operations. */}
          {error && (
            <div className='flex items-start gap-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'>
              <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
              <span className='flex-1 wrap-break-word'>{error}</span>
              <button
                type='button'
                className='shrink-0 hover:opacity-70 leading-none'
                onClick={clearError}
                aria-label='Dismiss error'
              >
                ×
              </button>
            </div>
          )}

          {/* Sync status + last fetched timestamp in a single footer row */}
          <div className='flex items-center justify-between text-xs text-muted-foreground'>
            <div className='flex items-center gap-1.5'>
              {isUpToDate ? (
                <Check className='h-3.5 w-3.5 text-emerald-500' />
              ) : behind > 0 ? (
                <AlertCircle className='h-3.5 w-3.5 text-amber-500' />
              ) : (
                <GitCommit className='h-3.5 w-3.5' />
              )}
              <span
                className={cn(isUpToDate ? 'text-emerald-500' : behind > 0 ? 'text-amber-500' : '')}
              >
                {isUpToDate
                  ? 'Up to date'
                  : behind > 0
                    ? `${behind} commit${behind > 1 ? 's' : ''} behind`
                    : `${ahead} commit${ahead > 1 ? 's' : ''} ahead`}
              </span>
            </div>
            <div className='flex items-center gap-1'>
              <Clock className='h-3 w-3' />
              <span>{lastFetched ?? 'Never fetched'}</span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Auto-stash confirmation dialog. */}
      <AlertDialog open={showStashDialog} onOpenChange={setShowStashDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Uncommitted Changes</AlertDialogTitle>
            <AlertDialogDescription>
              You have uncommitted changes. Pulling may cause conflicts or data loss. Would you like
              to stash your changes first?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className='flex-wrap gap-2'>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePullAnyway}>Pull Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleStashAndPull}>Stash & Pull</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Fetch-before-push confirmation dialog. */}
      <AlertDialog open={showFetchFirstDialog} onOpenChange={setShowFetchFirstDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Fetch Before Push</AlertDialogTitle>
            <AlertDialogDescription>
              {(status?.behind ?? 0) > 0
                ? `Your branch is ${status?.behind} commits behind the remote. Fetching first ensures you have the latest changes and reduces the risk of conflicts.`
                : 'You have not fetched from the remote yet. Fetching first ensures you have the latest changes and reduces the risk of conflicts.'}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className='flex-wrap gap-2'>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePushAnyway}>Push Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleFetchAndPush}>Fetch & Push</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Force-push confirmation dialog — distinct from the local-only reset warning. */}
      <AlertDialog open={showForcePushDialog} onOpenChange={setShowForcePushDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Force Push?</AlertDialogTitle>
            <AlertDialogDescription>
              This will overwrite{' '}
              <span className='font-mono'>
                {activeRemote ?? 'the remote'}/{status?.branch ?? 'this branch'}
              </span>
              's history to match your local <span className='font-mono'>{status?.branch}</span>.
              Anyone else who has fetched this branch may lose commits you don't have. This cannot
              be undone from the UI.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pushing}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className={cn(buttonVariants({ variant: 'destructive' }))}
              onClick={handleForcePush}
              disabled={pushing}
            >
              Force Push
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
