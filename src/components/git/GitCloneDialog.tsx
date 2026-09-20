import { Check, FolderOpen, Loader2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useOpenWorkspaceFromDisk, useSwitchWorkspace } from '@/lib/queries/workspace-queries';
import {
  type CloneDestinationGrant,
  type ClonedRepoStructure,
  type CollectionScanResult,
  detectClonedStructure,
  type GitCredentials,
  gitClone,
  selectCloneDestination,
} from '@/lib/tauri-api';
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';

type Step = 'input' | 'progress' | 'picker';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function GitCloneDialog({ open, onOpenChange }: Props) {
  const [step, setStep] = useState<Step>('input');
  const [repoUrl, setRepoUrl] = useState('');
  const [destination, setDestination] = useState<CloneDestinationGrant | null>(null);
  const [awaitingCredentials, setAwaitingCredentials] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [collections, setCollections] = useState<CollectionScanResult[]>([]);
  const [selectedCollection, setSelectedCollection] = useState<string | null>(null);

  // Bumped every time the dialog opens or a clone starts; a clone's async
  // continuation only touches state/navigation if it's still the current one
  // when it resolves. This stops a stale clone (dialog closed and reopened,
  // or reopened with a new URL/destination, while the old one was in flight)
  // from opening a workspace or writing into a dialog session it no longer owns.
  const requestIdRef = useRef(0);

  const credentials = useGitStore((s) => s.credentials);
  const gitStoreApi = useGitStoreApi();
  const openFromDiskMutation = useOpenWorkspaceFromDisk();
  const switchWorkspaceMutation = useSwitchWorkspace();

  // Reset all state when dialog opens.
  useEffect(() => {
    if (open) {
      requestIdRef.current += 1;
      setStep('input');
      setRepoUrl('');
      setDestination(null);
      setAwaitingCredentials(false);
      setError(null);
      setCollections([]);
      setSelectedCollection(null);
    } else {
      // Closing also disowns any clone still in flight from this session, and
      // any pending credentials-wait session — otherwise credentials arriving
      // after close would still trigger the continuation effect below.
      requestIdRef.current += 1;
      setAwaitingCredentials(false);
      setDestination(null);
    }
  }, [open]);

  const handleOpenWorkspace = useCallback(
    async (workspacePath: string) => {
      try {
        const ws = await openFromDiskMutation.mutateAsync(workspacePath);
        switchWorkspaceMutation.mutate(ws.id);
        onOpenChange(false);
      } catch (e) {
        setError(String(e));
        setStep('input');
      }
    },
    [onOpenChange, openFromDiskMutation, switchWorkspaceMutation],
  );

  // Handle post-clone detection: auto-open workspace or show picker.
  const handlePostClone = useCallback(
    async (clonedPath: string) => {
      const myRequestId = requestIdRef.current;
      const structure: ClonedRepoStructure = await detectClonedStructure(clonedPath);
      if (requestIdRef.current !== myRequestId) return;

      if (structure.kind === 'workspace' && structure.workspacePath) {
        await handleOpenWorkspace(structure.workspacePath);
        return;
      }

      if (structure.kind === 'collection' && structure.collections.length === 1) {
        await handleOpenWorkspace(structure.collections[0].path);
        return;
      }

      if (structure.collections.length > 0) {
        setCollections(structure.collections);
        setStep('picker');
        return;
      }

      // Nothing detected — show empty picker.
      setCollections([]);
      setStep('picker');
    },
    [handleOpenWorkspace],
  );

  const performClone = useCallback(
    async (creds: GitCredentials) => {
      if (!destination) {
        setError('Select an empty destination folder before cloning.');
        setStep('input');
        return;
      }

      const myRequestId = requestIdRef.current;
      setError(null);
      setStep('progress');
      try {
        await gitClone(repoUrl.trim(), destination.capability, creds);
        if (requestIdRef.current !== myRequestId) return;
        await handlePostClone(destination.displayPath);
      } catch (e) {
        if (requestIdRef.current !== myRequestId) return;
        // Capabilities are one-time, including failed clone attempts.
        setDestination(null);
        setError(String(e));
        setStep('input');
      }
    },
    [destination, handlePostClone, repoUrl],
  );

  // Continue exactly once when credentials arrive after the credentials dialog.
  useEffect(() => {
    if (!open) return;
    if (awaitingCredentials && credentials) {
      setAwaitingCredentials(false);
      void performClone(credentials);
    }
  }, [open, awaitingCredentials, credentials, performClone]);

  const handleBrowse = async () => {
    const result = await selectCloneDestination();
    if (result !== null) {
      setDestination(result);
    }
  };

  const handleClone = async () => {
    setError(null);
    const creds = gitStoreApi.getState().credentials;
    if (!creds) {
      setAwaitingCredentials(true);
      setStep('progress');
      gitStoreApi.getState().setShowCredentialsDialog(true);
      return;
    }
    await performClone(creds);
  };

  const handleOpen = async (collectionPath: string) => {
    await handleOpenWorkspace(collectionPath);
  };

  if (step === 'progress') {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle>Clone Repository</DialogTitle>
          </DialogHeader>
          <div className='flex flex-col items-center justify-center gap-3 min-h-30'>
            <Loader2 className='h-6 w-6 animate-spin text-muted-foreground' />
            <p className='text-sm text-muted-foreground'>Cloning repository...</p>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  if (step === 'picker') {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle>Clone Repository</DialogTitle>
            <DialogDescription>Repository cloned successfully.</DialogDescription>
          </DialogHeader>
          {collections.length === 0 ? (
            <>
              <p className='text-sm text-muted-foreground text-center py-4'>
                No collections found in this repository.
              </p>
              <DialogFooter>
                <Button size='sm' onClick={() => onOpenChange(false)}>
                  Close
                </Button>
              </DialogFooter>
            </>
          ) : collections.length === 1 ? (
            <>
              <p className='text-sm'>
                Found collection: <span className='font-medium'>{collections[0].name}</span>
              </p>
              <DialogFooter>
                <Button size='sm' onClick={() => handleOpen(collections[0].path)}>
                  Open
                </Button>
              </DialogFooter>
            </>
          ) : (
            <>
              <div className='space-y-1'>
                {collections.map((col) => (
                  <Button
                    key={col.path}
                    type='button'
                    variant='ghost'
                    className='flex items-center gap-2 px-2 py-1.5 h-auto rounded justify-start font-normal hover:bg-muted/50 text-sm w-full text-left'
                    onClick={() => setSelectedCollection(col.path)}
                  >
                    <Check
                      className='h-3 w-3 shrink-0'
                      style={{
                        opacity: selectedCollection === col.path ? 1 : 0,
                      }}
                    />
                    <span className='truncate'>{col.name}</span>
                  </Button>
                ))}
              </div>
              <DialogFooter>
                <Button
                  size='sm'
                  disabled={!selectedCollection}
                  onClick={() => selectedCollection && handleOpen(selectedCollection)}
                >
                  Open
                </Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
    );
  }

  // Default: input step.
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Clone Repository</DialogTitle>
        </DialogHeader>
        <div className='space-y-4'>
          <div>
            <Label className='text-sm'>Repository URL</Label>
            <Input
              placeholder='https://github.com/user/repo.git'
              aria-label='Repository URL'
              value={repoUrl}
              onChange={(e) => setRepoUrl(e.target.value)}
              className='h-8 text-sm'
            />
          </div>
          <div>
            <Label className='text-sm'>Destination</Label>
            <div className='flex gap-2'>
              <Input
                value={destination?.displayPath ?? ''}
                readOnly
                aria-label='Destination'
                placeholder='Select an empty folder'
                className='h-8 text-sm flex-1'
              />
              <Button variant='outline' size='sm' className='h-8 shrink-0' onClick={handleBrowse}>
                <FolderOpen className='h-3.5 w-3.5 mr-1' /> Browse
              </Button>
            </div>
          </div>
          {error && <p className='text-sm text-destructive wrap-break-word'>{error}</p>}
        </div>
        <DialogFooter>
          <Button size='sm' disabled={!repoUrl.trim() || !destination} onClick={handleClone}>
            Clone
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
