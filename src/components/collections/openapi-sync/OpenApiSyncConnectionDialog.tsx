import { ShieldAlert, ShieldCheck } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { SavedPill } from '@/components/ui/saved-pill';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import {
  useClearToken,
  useOpenApiSyncStatus,
  useRevokeConsent,
  useSetInterval,
  useSetToken,
} from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncInterval, OpenApiSyncStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { ChangeLinkForm } from './ChangeLinkForm';
import { CheckNowButtonView, useCheckRunner } from './CheckNowButton';
import { DisconnectSyncDialog } from './DisconnectSyncDialog';
import { OpenApiSyncConsentBanner } from './OpenApiSyncConsentBanner';
import { intervalLabel, isFileLocation, isGithubLocation, splitLocation } from './openApiSyncView';
import { syncErrorMessage } from './syncErrors';

const FREQUENCIES: OpenApiSyncInterval[] = [
  'every_5_minutes',
  'every_15_minutes',
  'every_30_minutes',
  'hourly',
  'daily',
  'weekly',
  'monthly',
];

const DEFAULT_FREQUENCY: OpenApiSyncInterval = 'every_5_minutes';

interface OpenApiSyncConnectionDialogProps {
  collection: string;
  status: OpenApiSyncStatus;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Puts the cursor in the access token field when the dialog opens. */
  focus?: 'token' | 'link';
}

function ConnectionBody({
  collection,
  status,
  onDisconnected,
  onChangingBusy,
  focus,
}: {
  collection: string;
  status: OpenApiSyncStatus;
  onDisconnected: () => void;
  onChangingBusy: (busy: boolean) => void;
  focus?: 'token' | 'link';
}) {
  const granted = status.consent === 'granted';
  const github = isGithubLocation(status.location);
  const unknown = status.intervalUnknown === true;
  const autoOn = status.interval !== 'off';
  // The token flag is only requested while this dialog is open.
  const tokenStatus = useOpenApiSyncStatus(collection, true);
  const hasToken = tokenStatus.data?.hasToken === true;
  const interval = useSetInterval(collection);
  const enabling = useRef(false);
  // The status has caught up with the switch, so the next turn on may check again.
  if (autoOn) enabling.current = false;
  // One runner serves the status row and the check that follows turning checks on.
  const runner = useCheckRunner(collection, status.consent);
  const setToken = useSetToken(collection);
  const clearToken = useClearToken(collection);
  const revoke = useRevokeConsent(collection);
  const [token, setTokenText] = useState('');
  const [tokenError, setTokenError] = useState<string | null>(null);
  const [showFull, setShowFull] = useState(false);
  const [needConsent, setNeedConsent] = useState(false);
  const [disconnectOpen, setDisconnectOpen] = useState(false);
  // Opened from Choose another file, the form for the new source is already open.
  const [changingLink, setChangingLink] = useState(focus === 'link');
  // The switch turns back on with the frequency the user picked last.
  const [lastChosen, setLastChosen] = useState<OpenApiSyncInterval>(
    status.interval !== 'off' ? status.interval : DEFAULT_FREQUENCY,
  );
  // A stored frequency that is not off wins, so a change made elsewhere is followed.
  if (status.interval !== 'off' && status.interval !== lastChosen) setLastChosen(status.interval);
  const { reset: resetSetToken } = setToken;
  const { short, full } = splitLocation(status.location);

  // The mutation keeps its variables, so it is reset when the field goes away.
  useEffect(() => resetSetToken, [resetSetToken]);

  const toggleAuto = (on: boolean) => {
    if (!granted) {
      // Nothing changes until the link is approved on this machine.
      setNeedConsent(true);
      return;
    }
    if (on && status.interval === 'off') {
      // Turning checks on from off checks once now, so the user sees the link works.
      // A second click before the status shows the new interval must not check again.
      if (enabling.current) return;
      enabling.current = true;
      interval.mutate(lastChosen, {
        onSuccess: () => runner.run(),
        onError: () => {
          enabling.current = false;
        },
      });
      return;
    }
    interval.mutate(on ? lastChosen : 'off');
  };
  const pickFrequency = (value: OpenApiSyncInterval) => {
    setLastChosen(value);
    interval.mutate(value);
  };

  const saveToken = () => {
    const value = token.trim();
    if (!value || setToken.isPending) return;
    // The text leaves the field state before the call, and the mutation is reset after it.
    setTokenText('');
    setTokenError(null);
    setToken.mutate(value, {
      onError: (e) => setTokenError(syncErrorMessage(e)),
      onSettled: () => resetSetToken(),
    });
  };

  const intervalError = interval.isError ? syncErrorMessage(interval.error) : null;
  const clearError = clearToken.isError ? syncErrorMessage(clearToken.error) : null;
  const describedBy =
    [unknown && 'openapi-sync-unknown', intervalError && 'openapi-sync-interval-error']
      .filter(Boolean)
      .join(' ') || undefined;
  const revokeError = revoke.isError ? syncErrorMessage(revoke.error) : null;

  const failed = status.lastStatus?.state === 'failed';
  // Five-minute checks use up GitHub's anonymous limit. It waits for the token flag to load.
  const showRateHint =
    granted &&
    github &&
    status.interval === 'every_5_minutes' &&
    tokenStatus.data !== undefined &&
    !hasToken;
  // "No problems" is only true after a check that ran and did not fail.
  const ran = status.lastStatus?.state === 'unchanged' || status.lastStatus?.state === 'changed';
  let statusSentence = '';
  if (failed) statusSentence = status.lastMessage ?? 'The last check failed.';
  else if (ran && granted) statusSentence = 'No problems.';
  const checkedText = status.lastCheckedAt
    ? `Last checked ${formatRelativeTime(status.lastCheckedAt)}.`
    : 'Not checked yet.';

  return (
    <>
      <section
        aria-label='Status'
        className='flex flex-wrap items-center justify-between gap-3 rounded-md border border-border/70 px-3 py-2'
      >
        <p className={cn('text-sm', failed && 'text-destructive')}>
          {checkedText} {statusSentence}
        </p>
        <CheckNowButtonView runner={runner} />
      </section>

      <section
        aria-label='Spec link'
        className='flex flex-col gap-2 rounded-lg border border-border/70 bg-muted/30 p-3'
      >
        <div className='flex items-start justify-between gap-3'>
          <p className='min-w-0 break-all text-sm font-medium leading-snug'>
            {isFileLocation(status.location) && (
              <Badge variant='secondary' className='mr-2 align-middle'>
                Local file
              </Badge>
            )}
            {full && showFull ? full : short}
          </p>
          <Badge variant={granted ? 'outline' : 'warning'} className='shrink-0 gap-1'>
            {granted ? (
              <ShieldCheck aria-hidden='true' className='h-3 w-3' />
            ) : (
              <ShieldAlert aria-hidden='true' className='h-3 w-3' />
            )}
            {granted ? 'Approved on this machine' : 'Not approved yet'}
          </Badge>
        </div>
        <div className='flex flex-wrap items-center gap-3'>
          {full && (
            <Button
              variant='link'
              size='sm'
              className='h-auto p-0 text-xs'
              onClick={() => setShowFull((v) => !v)}
            >
              {showFull ? 'Hide full link' : 'Show full link'}
            </Button>
          )}
          {!changingLink && (
            <Button
              variant='link'
              size='sm'
              className='h-auto p-0 text-xs'
              onClick={() => setChangingLink(true)}
            >
              Change link
            </Button>
          )}
        </div>
        {changingLink && (
          <ChangeLinkForm
            collection={collection}
            currentLocation={status.location}
            onDone={() => setChangingLink(false)}
            onBusyChange={onChangingBusy}
          />
        )}
      </section>

      {needConsent && status.consent !== 'granted' && (
        <OpenApiSyncConsentBanner
          key={collection}
          collection={collection}
          location={status.location}
          consent={status.consent}
        />
      )}

      <section aria-label='Automatic checks' className='flex flex-col gap-3'>
        <div className='flex items-center justify-between gap-4'>
          <div className='flex min-w-0 flex-col gap-0.5'>
            <Label htmlFor='openapi-sync-auto' className='text-sm font-medium'>
              Check for spec updates automatically
            </Label>
            <p className='text-xs text-muted-foreground'>
              Rocket asks the link for changes in the background.
            </p>
          </div>
          <Switch
            id='openapi-sync-auto'
            checked={autoOn}
            onCheckedChange={toggleAuto}
            disabled={interval.isPending}
          />
        </div>
        <div className='flex flex-col gap-1.5'>
          <div className='flex items-center justify-between gap-4'>
            <Label htmlFor='openapi-sync-interval' className='text-sm'>
              Check frequency
            </Label>
            <Select
              value={unknown ? '' : lastChosen}
              onValueChange={(v) => pickFrequency(v as OpenApiSyncInterval)}
              disabled={(!autoOn && !unknown) || !granted || interval.isPending}
            >
              <SelectTrigger
                id='openapi-sync-interval'
                aria-label='Check frequency'
                aria-describedby={describedBy}
                className='w-48'
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {FREQUENCIES.map((i) => (
                  <SelectItem key={i} value={i}>
                    {intervalLabel(i)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {showRateHint && (
            <p className='text-xs text-muted-foreground'>
              GitHub allows 60 requests an hour without a token. Pick a longer interval or add a
              token.{' '}
              <Button
                variant='link'
                size='sm'
                className='h-auto p-0 text-xs'
                onClick={() => document.getElementById('openapi-sync-token')?.focus()}
              >
                Add token
              </Button>
            </p>
          )}
          {unknown && (
            <p id='openapi-sync-unknown' className='text-xs text-muted-foreground'>
              This setting is not understood. Pick a frequency.
            </p>
          )}
          {!granted && (
            <p className='text-xs text-muted-foreground'>
              Approve the spec link to set a frequency. The stored frequency and token come back
              when the link is approved again.
            </p>
          )}
          {intervalError && (
            <p id='openapi-sync-interval-error' role='alert' className='text-xs text-destructive'>
              {intervalError}
            </p>
          )}
        </div>
      </section>

      <section aria-label='Access' className='flex flex-col gap-2 border-t border-border/70 pt-4'>
        {github ? (
          <>
            <div className='flex items-center justify-between gap-3'>
              <Label htmlFor='openapi-sync-token' className='text-sm font-medium'>
                Access token
              </Label>
              {hasToken && (
                <div className='flex items-center gap-2'>
                  <SavedPill persistent>A token is stored</SavedPill>
                  <Button
                    size='sm'
                    variant='outline'
                    className='h-7'
                    onClick={() => clearToken.mutate()}
                    disabled={clearToken.isPending}
                  >
                    Clear
                  </Button>
                </div>
              )}
            </div>
            <p className='text-xs text-muted-foreground'>
              Tokens work with GitHub links only. The token is kept in the keychain and never shown.
            </p>
            <div className='flex items-center gap-2'>
              <Input
                id='openapi-sync-token'
                type='password'
                autoComplete='new-password'
                aria-describedby={tokenError ? 'openapi-sync-token-error' : undefined}
                value={token}
                onChange={(e) => setTokenText(e.target.value)}
                placeholder={hasToken ? 'Paste a new token to replace it' : 'Paste the token'}
                className='h-8 flex-1 text-sm'
              />
              <Button
                size='sm'
                onClick={saveToken}
                disabled={token.trim() === '' || setToken.isPending}
              >
                Save token
              </Button>
            </div>
            {tokenError && (
              <p id='openapi-sync-token-error' role='alert' className='text-xs text-destructive'>
                {tokenError}
              </p>
            )}
            {clearError && (
              <p role='alert' className='text-xs text-destructive'>
                {clearError}
              </p>
            )}
          </>
        ) : (
          <p className='text-xs text-muted-foreground'>Tokens work with GitHub links only.</p>
        )}
      </section>

      <section
        aria-label='Stop or remove'
        className='flex flex-col gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-3'
      >
        {granted && (
          <div className='flex flex-col gap-1.5'>
            <div className='flex items-center justify-between gap-4'>
              <p className='min-w-0 text-xs text-muted-foreground'>
                Stops background checks and token use until you approve the link again.
              </p>
              <Button
                size='sm'
                variant='outline'
                className='shrink-0'
                onClick={() => revoke.mutate()}
                disabled={revoke.isPending}
              >
                Stop trusting this link
              </Button>
            </div>
            {revokeError && (
              <p role='alert' className='text-xs text-destructive'>
                {revokeError}
              </p>
            )}
          </div>
        )}
        <div className='flex items-center justify-between gap-4'>
          <p className='min-w-0 text-xs text-muted-foreground'>
            Removes the sync link from this collection. Your requests stay.
          </p>
          <Button
            size='sm'
            variant='destructive'
            className='shrink-0'
            onClick={() => setDisconnectOpen(true)}
          >
            Disconnect
          </Button>
        </div>
      </section>
      <DisconnectSyncDialog
        collection={collection}
        open={disconnectOpen}
        onOpenChange={setDisconnectOpen}
        onDisconnected={onDisconnected}
      />
    </>
  );
}

/** Frequency, token, trust and disconnect for the spec link, in a dialog. */
export function OpenApiSyncConnectionDialog({
  collection,
  status,
  open,
  onOpenChange,
  focus,
}: OpenApiSyncConnectionDialogProps) {
  // A link change cannot be cancelled once sent, so the dialog stays until it answers.
  const [changing, setChanging] = useState(false);
  const requestClose = (next: boolean) => {
    if (!next && changing) return;
    onOpenChange(next);
  };
  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent
        onOpenAutoFocus={(event) => {
          if (focus !== 'token') return;
          const input = document.getElementById('openapi-sync-token');
          if (!input) return;
          event.preventDefault();
          input.scrollIntoView?.({ block: 'center' });
          input.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Connection settings</DialogTitle>
          <DialogDescription>
            How Rocket follows the spec this collection came from.
          </DialogDescription>
        </DialogHeader>
        <DialogBody className='flex flex-col gap-5'>
          <ConnectionBody
            collection={collection}
            status={status}
            onDisconnected={() => onOpenChange(false)}
            onChangingBusy={setChanging}
            focus={focus}
          />
        </DialogBody>
      </DialogContent>
    </Dialog>
  );
}
