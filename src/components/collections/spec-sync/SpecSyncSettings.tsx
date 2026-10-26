import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { CollapsibleSection } from '@/components/ui/collapsible-section';
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
import {
  useClearToken,
  useOpenApiSyncStatus,
  useRevokeConsent,
  useSetInterval,
  useSetToken,
} from '@/lib/queries/openapi-sync-queries';
import type { OpenApiSyncInterval, OpenApiSyncStatus } from '@/lib/tauri-api';
import { intervalLabel, isGithubLocation } from './specSyncView';
import { syncErrorMessage } from './syncErrors';

const INTERVALS: OpenApiSyncInterval[] = [
  'off',
  'every_5_minutes',
  'every_15_minutes',
  'every_30_minutes',
  'hourly',
  'daily',
  'weekly',
  'monthly',
];

interface SpecSyncSettingsProps {
  collection: string;
  status: OpenApiSyncStatus;
}

function SettingsBody({ collection, status }: SpecSyncSettingsProps) {
  const granted = status.consent === 'granted';
  const github = isGithubLocation(status.location);
  // The token flag is only requested while this section is mounted.
  const tokenStatus = useOpenApiSyncStatus(collection, true);
  const hasToken = tokenStatus.data?.hasToken === true;
  const interval = useSetInterval(collection);
  const setToken = useSetToken(collection);
  const clearToken = useClearToken(collection);
  const revoke = useRevokeConsent(collection);
  const [token, setTokenText] = useState('');
  const [tokenError, setTokenError] = useState<string | null>(null);
  const { reset: resetSetToken } = setToken;

  // The mutation keeps its variables, so it is reset when the field goes away.
  useEffect(() => resetSetToken, [resetSetToken]);

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
        <Label htmlFor='spec-sync-interval'>Check interval</Label>
        <Select
          value={status.interval}
          onValueChange={(v) => interval.mutate(v as OpenApiSyncInterval)}
          disabled={!granted || interval.isPending}
        >
          <SelectTrigger id='spec-sync-interval' aria-label='Check interval' className='w-48'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {INTERVALS.map((i) => (
              <SelectItem key={i} value={i}>
                {intervalLabel(i)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {!granted && (
          <p className='text-xs text-muted-foreground'>
            Approve the spec link to set an interval. The stored interval and token come back when
            the link is approved again.
          </p>
        )}
        {intervalError && (
          <p role='alert' className='text-xs text-destructive'>
            {intervalError}
          </p>
        )}
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
    </>
  );
}

/** Interval, token and trust for the spec link, folded away until opened. */
export function SpecSyncSettings({ collection, status }: SpecSyncSettingsProps) {
  return (
    <div className='mx-4 mt-3'>
      <CollapsibleSection title='Settings'>
        <SettingsBody collection={collection} status={status} />
      </CollapsibleSection>
    </div>
  );
}
