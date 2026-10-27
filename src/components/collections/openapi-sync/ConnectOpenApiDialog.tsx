import { Loader2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import {
  useConfirmOpenApiConnect,
  usePreviewOpenApiConnect,
} from '@/lib/queries/openapi-sync-queries';
import type { ConnectPreview, OpenApiGroupBy } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { ConnectReviewStep } from './ConnectReviewStep';
import {
  type ConnectChoice,
  type ConnectRowChoice,
  defaultRows,
  hasUnresolvedAmbiguity,
  rowsToRequest,
} from './connectRows';
import { isGithubLocation } from './openApiSyncView';
import { connectWarningMessage, isStaleConnect, syncErrorMessage } from './syncErrors';

/** The interval the import dialog also turns on for "Check for spec updates". */
const DEFAULT_INTERVAL = 'every_5_minutes';

interface ConnectOpenApiDialogProps {
  collection: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Connects a hand-made collection to an OpenAPI spec: the link first, then the Initial sync review. */
export function ConnectOpenApiDialog({
  collection,
  open,
  onOpenChange,
}: ConnectOpenApiDialogProps) {
  const [link, setLink] = useState('');
  const [token, setToken] = useState('');
  const [groupBy, setGroupBy] = useState<OpenApiGroupBy>('tags');
  const [checks, setChecks] = useState(true);
  const [preview, setPreview] = useState<ConnectPreview | null>(null);
  const [rows, setRows] = useState<ConnectRowChoice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const previewMutation = usePreviewOpenApiConnect(collection);
  const confirmMutation = useConfirmOpenApiConnect(collection);
  // A ref so a fast second click is dropped before the mutation state has rerendered.
  const confirming = useRef(false);
  // Bumped on close, so a call that finishes later is dropped and cannot fill a reopened dialog.
  const epoch = useRef(0);

  // Closing drops everything, the token included, so a reopened dialog starts clean.
  useEffect(() => {
    if (open) return;
    setLink('');
    setToken('');
    setGroupBy('tags');
    setChecks(true);
    setPreview(null);
    setRows([]);
    setError(null);
    confirming.current = false;
    epoch.current += 1;
    previewMutation.reset();
    confirmMutation.reset();
  }, [open, previewMutation.reset, confirmMutation.reset]);

  const trimmed = link.trim();
  const github = isGithubLocation(trimmed);

  const runPreview = async () => {
    setError(null);
    const started = epoch.current;
    try {
      const result = await previewMutation.mutateAsync({
        link: trimmed,
        token: github && token.trim() ? token.trim() : undefined,
      });
      if (started !== epoch.current) return;
      setPreview(result);
      setRows(defaultRows(result));
    } catch (e) {
      if (started === epoch.current) setError(syncErrorMessage(e));
    }
  };

  const connect = async () => {
    if (!preview || confirming.current) return;
    confirming.current = true;
    setError(null);
    const started = epoch.current;
    try {
      const outcome = await confirmMutation.mutateAsync({
        link: trimmed,
        token: github && token.trim() ? token.trim() : undefined,
        rows: rowsToRequest(rows),
        groupBy,
        interval: checks ? DEFAULT_INTERVAL : undefined,
        digest: preview.digest,
        specSha256: preview.specSha256,
      });
      for (const warning of outcome.warnings) {
        const sentence = connectWarningMessage(warning);
        if (sentence) toast.info(sentence);
      }
      onOpenChange(false);
      usePaneStore.getState().openOpenApiSyncTab(collection);
    } catch (e) {
      if (started !== epoch.current) return;
      confirming.current = false;
      setError(syncErrorMessage(e));
      // A stale preview cannot be confirmed, so the user goes back to run it again.
      if (isStaleConnect(e)) {
        setPreview(null);
        setRows([]);
      }
    }
  };

  const onChoice = (id: string, choice: ConnectChoice) =>
    setRows((current) => current.map((r) => (r.id === id ? { ...r, choice } : r)));

  const busy = previewMutation.isPending || confirmMutation.isPending;

  // The connect cannot be cancelled once sent, so the dialog stays until it answers.
  const requestClose = (next: boolean) => {
    if (!next && confirmMutation.isPending) return;
    onOpenChange(next);
  };

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent className='max-w-3xl'>
        <DialogHeader>
          <DialogTitle>Connect to an OpenAPI spec</DialogTitle>
          <DialogDescription>
            {preview
              ? `Initial sync review: ${preview.specTitle}${preview.specVersion ? ` ${preview.specVersion}` : ''}`
              : 'Link this collection to a spec so Rocket can keep it up to date. Nothing is changed until you connect.'}
          </DialogDescription>
        </DialogHeader>
        <DialogBody className='flex max-h-[60vh] flex-col gap-4 overflow-y-auto'>
          {preview ? (
            <ConnectReviewStep rows={rows} onChange={onChoice} />
          ) : (
            <div className='space-y-3'>
              <Input
                value={link}
                onChange={(e) => setLink(e.target.value)}
                placeholder='https://github.com/owner/repo/blob/main/openapi.yaml'
                aria-label='Spec link'
              />
              <p className='text-[11px] text-muted-foreground'>
                An https link, or http://localhost for a local server.
              </p>
              {github && (
                <div className='space-y-1'>
                  <Label htmlFor='connect-openapi-token' className='text-xs font-medium'>
                    Access token (optional)
                  </Label>
                  <Input
                    id='connect-openapi-token'
                    type='password'
                    autoComplete='off'
                    value={token}
                    onChange={(e) => setToken(e.target.value)}
                  />
                  <p className='text-[11px] text-muted-foreground'>
                    Only for private GitHub repositories. Kept in your system keychain, never in the
                    collection.
                  </p>
                </div>
              )}
              <div className='space-y-1.5'>
                <Label className='text-xs font-medium text-muted-foreground'>
                  Group new folders by
                </Label>
                <RadioGroup
                  value={groupBy}
                  onValueChange={(v) => setGroupBy(v as OpenApiGroupBy)}
                  className='flex gap-4'
                  aria-label='Group new folders by'
                >
                  <div className='flex items-center gap-1.5'>
                    <RadioGroupItem value='tags' id='connect-group-tags' />
                    <Label htmlFor='connect-group-tags' className='text-xs'>
                      Tag
                    </Label>
                  </div>
                  <div className='flex items-center gap-1.5'>
                    <RadioGroupItem value='paths' id='connect-group-paths' />
                    <Label htmlFor='connect-group-paths' className='text-xs'>
                      Path
                    </Label>
                  </div>
                </RadioGroup>
              </div>
              <div className='flex items-center gap-2'>
                <Checkbox
                  id='connect-check-updates'
                  checked={checks}
                  onCheckedChange={(v) => setChecks(v === true)}
                />
                <Label htmlFor='connect-check-updates' className='text-xs'>
                  Check for spec updates
                </Label>
              </div>
            </div>
          )}
          {error && (
            <p role='alert' className='text-sm text-destructive'>
              {error}
            </p>
          )}
        </DialogBody>
        <DialogFooter>
          <Button
            variant='ghost'
            disabled={confirmMutation.isPending}
            onClick={() => requestClose(false)}
          >
            Cancel
          </Button>
          {preview ? (
            <>
              <Button variant='outline' disabled={busy} onClick={() => setPreview(null)}>
                Back
              </Button>
              <Button disabled={busy || hasUnresolvedAmbiguity(rows)} onClick={connect}>
                {confirmMutation.isPending && (
                  <Loader2 aria-hidden='true' className='mr-2 h-4 w-4 animate-spin' />
                )}
                Connect
              </Button>
            </>
          ) : (
            <Button disabled={busy || trimmed === ''} onClick={runPreview}>
              {previewMutation.isPending && (
                <Loader2 aria-hidden='true' className='mr-2 h-4 w-4 animate-spin' />
              )}
              Preview
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
