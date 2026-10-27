import { Loader2, TriangleAlert } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useConfirmChangeLink, usePreviewChangeLink } from '@/lib/queries/openapi-sync-queries';
import type { OpenApiChangeLinkPreview } from '@/lib/tauri-api';
import { isGithubLocation, splitLocation } from './openApiSyncView';
import { SpecSourceField } from './SpecSourceField';
import { changeLinkWarningMessage, isStaleConnect, syncErrorMessage } from './syncErrors';

interface ChangeLinkFormProps {
  collection: string;
  currentLocation: string;
  /** Called when the link was changed or the user cancelled. */
  onDone: () => void;
  /** Tells the dialog that a change is in flight, so it cannot be closed under it. */
  onBusyChange?: (busy: boolean) => void;
}

/** Fewer than half of the connected endpoints in the new spec is probably the wrong link. */
function isLowMatch(preview: OpenApiChangeLinkPreview): boolean {
  const total = preview.matched + preview.removed;
  return total > 0 && preview.matched * 2 < total;
}

/** Moves the collection to another spec link: check the link first, then change it. */
export function ChangeLinkForm({
  collection,
  currentLocation,
  onDone,
  onBusyChange,
}: ChangeLinkFormProps) {
  const [link, setLink] = useState('');
  const [token, setToken] = useState('');
  const [preview, setPreview] = useState<OpenApiChangeLinkPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const previewMutation = usePreviewChangeLink(collection);
  const confirmMutation = useConfirmChangeLink(collection);
  // A ref so a fast second click is dropped before the state has rerendered.
  const sent = useRef(false);
  // Bumped on unmount, so a call that finishes later cannot touch a form that is gone.
  const epoch = useRef(0);

  useEffect(
    () => () => {
      epoch.current += 1;
    },
    [],
  );
  useEffect(() => {
    onBusyChange?.(confirming);
    return () => onBusyChange?.(false);
  }, [confirming, onBusyChange]);

  const trimmed = link.trim();
  const github = isGithubLocation(trimmed);
  const tokenValue = github && token.trim() ? token.trim() : undefined;

  const edit = (value: string) => {
    setLink(value);
    // A new link needs its own check.
    setPreview(null);
    setError(null);
  };

  const check = async () => {
    setError(null);
    setPreview(null);
    const started = epoch.current;
    try {
      const result = await previewMutation.mutateAsync({ link: trimmed, token: tokenValue });
      if (started === epoch.current) setPreview(result);
    } catch (e) {
      if (started === epoch.current) setError(syncErrorMessage(e));
    }
  };

  const change = async () => {
    if (!preview || sent.current) return;
    sent.current = true;
    setConfirming(true);
    setError(null);
    const started = epoch.current;
    try {
      const outcome = await confirmMutation.mutateAsync({
        link: trimmed,
        token: tokenValue,
        specSha256: preview.specSha256,
      });
      toast.success('Link changed.');
      for (const warning of outcome.warnings) {
        const sentence = changeLinkWarningMessage(warning);
        if (sentence) toast.info(sentence);
      }
      setConfirming(false);
      onDone();
    } catch (e) {
      if (started !== epoch.current) return;
      sent.current = false;
      setConfirming(false);
      setError(syncErrorMessage(e));
      // A stale check cannot be confirmed, so the user checks the link again.
      if (isStaleConnect(e)) setPreview(null);
    }
  };

  const total = preview ? preview.matched + preview.removed : 0;
  const checking = previewMutation.isPending;

  return (
    <div className='flex flex-col gap-3 rounded-lg border border-border/70 p-3'>
      <div className='space-y-1'>
        <Label htmlFor='change-link-input' className='text-xs font-medium'>
          New spec link
        </Label>
        <SpecSourceField value={link} onChange={edit} label='New spec link' disabled={confirming} />
        <p className='text-[11px] text-muted-foreground'>
          Now: {splitLocation(currentLocation).short}. Your requests, skips and backups stay.
        </p>
      </div>
      {github && (
        <div className='space-y-1'>
          <Label htmlFor='change-link-token' className='text-xs font-medium'>
            Access token (optional)
          </Label>
          <Input
            id='change-link-token'
            aria-label='Access token (optional)'
            type='password'
            autoComplete='off'
            value={token}
            onChange={(e) => setToken(e.target.value)}
            disabled={confirming}
          />
          <p className='text-[11px] text-muted-foreground'>
            Only for private GitHub repositories. A token saved for another repository is removed.
          </p>
        </div>
      )}
      {preview && (
        <div className='space-y-1.5 text-xs'>
          {preview.sameSpec ? (
            <p>The spec is the same.</p>
          ) : total === 0 ? (
            <p>
              This spec has no connected endpoints to compare. It will be reviewed in Spec updates.
            </p>
          ) : (
            <p>
              {preview.matched} of {total} endpoints match. A new spec will be reviewed in Spec
              updates.
            </p>
          )}
          {isLowMatch(preview) && (
            <p
              role='status'
              className='flex items-start gap-1.5 text-amber-600 dark:text-amber-400'
            >
              <TriangleAlert aria-hidden='true' className='mt-0.5 h-3.5 w-3.5 shrink-0' />
              Only {preview.matched} of {total} endpoints match this spec. Check that this is the
              right link.
            </p>
          )}
        </div>
      )}
      {error && (
        <p role='alert' className='text-xs text-destructive'>
          {error}
        </p>
      )}
      <div className='flex flex-wrap items-center justify-end gap-2'>
        <Button size='sm' variant='ghost' onClick={onDone} disabled={confirming}>
          Cancel
        </Button>
        {preview ? (
          <Button size='sm' onClick={() => void change()} disabled={confirming}>
            {confirming && <Loader2 aria-hidden='true' className='h-3.5 w-3.5 animate-spin' />}
            Change link
          </Button>
        ) : (
          <Button
            size='sm'
            variant='outline'
            onClick={() => void check()}
            disabled={!trimmed || checking}
          >
            {checking && <Loader2 aria-hidden='true' className='h-3.5 w-3.5 animate-spin' />}
            Check link
          </Button>
        )}
      </div>
    </div>
  );
}
