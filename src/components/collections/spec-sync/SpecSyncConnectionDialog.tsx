import { useEffect, useState } from 'react';
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
import type { OpenApiSyncInterval, OpenApiSyncStatus } from '@/lib/tauri-api';
import { DisconnectSyncDialog } from './DisconnectSyncDialog';
import { SpecSyncConsentBanner } from './SpecSyncConsentBanner';
import { intervalLabel, isGithubLocation, splitLocation } from './specSyncView';
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

interface SpecSyncConnectionDialogProps {
  collection: string;
  status: OpenApiSyncStatus;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function ConnectionBody({
  collection,
  status,
  onDisconnected,
}: {
  collection: string;
  status: OpenApiSyncStatus;
  onDisconnected: () => void;
}) {
  const granted = status.consent === 'granted';
  const github = isGithubLocation(status.location);
  const unknown = status.intervalUnknown === true;
  const autoOn = status.interval !== 'off' || unknown;
  // The token flag is only requested while this dialog is open.
  const tokenStatus = useOpenApiSyncStatus(collection, true);
  const hasToken = tokenStatus.data?.hasToken === true;
  const interval = useSetInterval(collection);
  const setToken = useSetToken(collection);
  const clearToken = useClearToken(collection);
  const revoke = useRevokeConsent(collection);
  const [token, setTokenText] = useState('');
  const [tokenError, setTokenError] = useState<string | null>(null);
  const [showFull, setShowFull] = useState(false);
  const [needConsent, setNeedConsent] = useState(false);
  const [disconnectOpen, setDisconnectOpen] = useState(false);
  // The switch turns back on with the frequency the user picked last.
  const [lastChosen, setLastChosen] = useState<OpenApiSyncInterval>(
    status.interval !== 'off' ? status.interval : DEFAULT_FREQUENCY,
  );
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
  const revokeError = revoke.isError ? syncErrorMessage(revoke.error) : null;

  return (
    <>
      <div className='flex flex-col gap-1.5'>
        <p className='break-all text-sm font-medium'>{full && showFull ? full : short}</p>
        {full && (
          <div>
            <Button
              variant='link'
              size='sm'
              className='h-auto p-0 text-xs'
              onClick={() => setShowFull((v) => !v)}
            >
              {showFull ? 'Hide full link' : 'Show full link'}
            </Button>
          </div>
        )}
      </div>

      {needConsent && status.consent !== 'granted' && (
        <SpecSyncConsentBanner
          key={collection}
          collection={collection}
          location={status.location}
          consent={status.consent}
        />
      )}

      <div className='flex flex-col gap-3'>
        <div className='flex items-center gap-2'>
          <Switch
            id='spec-sync-auto'
            checked={autoOn}
            onCheckedChange={toggleAuto}
            disabled={interval.isPending}
          />
          <Label htmlFor='spec-sync-auto'>Check for spec updates automatically</Label>
        </div>
        <div className='flex flex-col gap-1.5'>
          <Label htmlFor='spec-sync-interval'>Check frequency</Label>
          <Select
            value={unknown ? '' : autoOn ? status.interval : ''}
            onValueChange={(v) => pickFrequency(v as OpenApiSyncInterval)}
            disabled={!autoOn || !granted || interval.isPending}
          >
            <SelectTrigger id='spec-sync-interval' aria-label='Check frequency' className='w-48'>
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
          {unknown && (
            <p className='text-xs text-muted-foreground'>
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
            <p role='alert' className='text-xs text-destructive'>
              {intervalError}
            </p>
          )}
        </div>
      </div>

      {github ? (
        <div className='flex flex-col gap-1.5'>
          <Label htmlFor='spec-sync-token'>Access token</Label>
          <p className='text-xs text-muted-foreground'>
            Tokens work with GitHub links only. The token is kept in the keychain and never shown.
          </p>
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
          <div className='flex items-center gap-2'>
            <Input
              id='spec-sync-token'
              type='password'
              autoComplete='new-password'
              aria-describedby={tokenError ? 'spec-sync-token-error' : undefined}
              value={token}
              onChange={(e) => setTokenText(e.target.value)}
              placeholder={hasToken ? 'Paste a new token to replace it' : 'Paste the token'}
              className='h-8 max-w-sm text-sm'
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
            <p id='spec-sync-token-error' role='alert' className='text-xs text-destructive'>
              {tokenError}
            </p>
          )}
          {clearError && (
            <p role='alert' className='text-xs text-destructive'>
              {clearError}
            </p>
          )}
        </div>
      ) : (
        <p className='text-xs text-muted-foreground'>Tokens work with GitHub links only.</p>
      )}

      {granted && (
        <div className='flex flex-col gap-1.5'>
          <div>
            <Button
              size='sm'
              variant='outline'
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

      <div className='border-t border-border/70 pt-3'>
        <Button size='sm' variant='destructive' onClick={() => setDisconnectOpen(true)}>
          Disconnect
        </Button>
      </div>
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
export function SpecSyncConnectionDialog({
  collection,
  status,
  open,
  onOpenChange,
}: SpecSyncConnectionDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit connection settings</DialogTitle>
          <DialogDescription>
            How Rocket follows the spec this collection came from.
          </DialogDescription>
        </DialogHeader>
        <DialogBody className='flex flex-col gap-4'>
          <ConnectionBody
            collection={collection}
            status={status}
            onDisconnected={() => onOpenChange(false)}
          />
        </DialogBody>
      </DialogContent>
    </Dialog>
  );
}
