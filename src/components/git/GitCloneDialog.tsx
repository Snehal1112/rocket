import { useState, useEffect } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Check, FolderOpen, Loader2 } from 'lucide-react';
import { useGitStore } from '@/stores/git-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { gitClone, openFolderPicker, scanCollectionsInPath, type CollectionScanResult } from '@/lib/tauri-api';

type Step = 'input' | 'progress' | 'picker';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function GitCloneDialog({ open, onOpenChange }: Props) {
  const [step, setStep] = useState<Step>('input');
  const [repoUrl, setRepoUrl] = useState('');
  const [destPath, setDestPath] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [collections, setCollections] = useState<CollectionScanResult[]>([]);
  const [selectedCollection, setSelectedCollection] = useState<string | null>(null);

  const credentials = useGitStore((s) => s.credentials);

  // Reset all state when dialog opens.
  useEffect(() => {
    if (open) {
      setStep('input');
      setRepoUrl('');
      setDestPath('');
      setError(null);
      setCollections([]);
      setSelectedCollection(null);
    }
  }, [open]);

  // Retry clone when credentials arrive while waiting in progress step.
  useEffect(() => {
    if (step === 'progress' && credentials) {
      const doClone = async () => {
        try {
          await gitClone(repoUrl.trim(), destPath.trim(), credentials);
          const found = await scanCollectionsInPath(destPath.trim());
          setCollections(found);
          setStep('picker');
        } catch (e) {
          setError(String(e));
          setStep('input');
        }
      };
      void doClone();
    }
  }, [credentials, step]);

  const handleBrowse = async () => {
    const result = await openFolderPicker();
    if (result !== null) {
      setDestPath(result);
    }
  };

  const handleClone = async () => {
    setError(null);
    setStep('progress');
    const creds = useGitStore.getState().credentials;
    if (!creds) {
      useGitStore.getState().setShowCredentialsDialog(true);
      return;
    }
    try {
      await gitClone(repoUrl.trim(), destPath.trim(), creds);
      const found = await scanCollectionsInPath(destPath.trim());
      setCollections(found);
      setStep('picker');
    } catch (e) {
      setError(String(e));
      setStep('input');
    }
  };

  const handleOpen = async (collectionPath: string) => {
    try {
      await useWorkspaceStore.getState().openWorkspaceFromDisk(collectionPath);
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
    }
  };

  if (step === 'progress') {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Clone Repository</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col items-center justify-center gap-3 min-h-[120px]">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            <p className="text-sm text-muted-foreground">Cloning repository...</p>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  if (step === 'picker') {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Clone Repository</DialogTitle>
            <DialogDescription>Repository cloned successfully.</DialogDescription>
          </DialogHeader>
          {collections.length === 0 ? (
            <>
              <p className="text-sm text-muted-foreground text-center py-4">
                No collections found in this repository.
              </p>
              <DialogFooter>
                <Button size="sm" onClick={() => onOpenChange(false)}>Close</Button>
              </DialogFooter>
            </>
          ) : collections.length === 1 ? (
            <>
              <p className="text-sm">
                Found collection: <span className="font-medium">{collections[0].name}</span>
              </p>
              <DialogFooter>
                <Button size="sm" onClick={() => handleOpen(collections[0].path)}>Open</Button>
              </DialogFooter>
            </>
          ) : (
            <>
              <div className="space-y-1">
                {collections.map((col) => (
                  <div
                    key={col.path}
                    className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-muted/50 cursor-pointer text-sm"
                    onClick={() => setSelectedCollection(col.path)}
                  >
                    <Check
                      className="h-3 w-3 shrink-0"
                      style={{ opacity: selectedCollection === col.path ? 1 : 0 }}
                    />
                    <span className="truncate">{col.name}</span>
                  </div>
                ))}
              </div>
              <DialogFooter>
                <Button
                  size="sm"
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
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Clone Repository</DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          <div>
            <Label className="text-sm">Repository URL</Label>
            <Input
              placeholder="https://github.com/user/repo.git"
              value={repoUrl}
              onChange={(e) => setRepoUrl(e.target.value)}
              className="h-8 text-sm"
            />
          </div>
          <div>
            <Label className="text-sm">Destination</Label>
            <div className="flex gap-2">
              <Input
                value={destPath}
                onChange={(e) => setDestPath(e.target.value)}
                className="h-8 text-sm flex-1"
              />
              <Button variant="outline" size="sm" className="h-8 shrink-0" onClick={handleBrowse}>
                <FolderOpen className="h-3.5 w-3.5 mr-1" /> Browse
              </Button>
            </div>
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
        <DialogFooter>
          <Button size="sm" disabled={!repoUrl.trim() || !destPath.trim()} onClick={handleClone}>
            Clone
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
