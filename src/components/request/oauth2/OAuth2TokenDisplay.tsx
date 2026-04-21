import { ChevronDown, ChevronRight, Copy, Key } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface OAuth2TokenDisplayProps {
  oauth2: OAuth2State;
}

function tokenExpiryDisplay(expiresIn: number | null, acquiredAt: number | null): string {
  if (!expiresIn || !acquiredAt) return 'No expiry';
  const expiresAt = acquiredAt + expiresIn;
  const now = Math.floor(Date.now() / 1000);
  if (now >= expiresAt) return 'Expired';
  const remaining = expiresAt - now;
  const date = new Date(expiresAt * 1000);
  const time = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (remaining < 60) return `Expires in ${remaining}s (at ${time})`;
  if (remaining < 3600) return `Expires in ${Math.floor(remaining / 60)}m (at ${time})`;
  return `Expires in ${Math.floor(remaining / 3600)}h ${Math.floor((remaining % 3600) / 60)}m`;
}

function isExpired(expiresIn: number | null, acquiredAt: number | null): boolean {
  if (!expiresIn || !acquiredAt) return false;
  return Math.floor(Date.now() / 1000) >= acquiredAt + expiresIn;
}

function formatTimestamp(ts: number | null): string {
  if (!ts) return '—';
  return new Date(ts * 1000).toLocaleString();
}

export function OAuth2TokenDisplay({ oauth2: o }: OAuth2TokenDisplayProps) {
  const [accessOpen, setAccessOpen] = useState(false);
  const [idOpen, setIdOpen] = useState(false);
  const [showRawPayload, setShowRawPayload] = useState(false);
  const [, setTick] = useState(0);

  // Re-render every 30s to keep the expiry countdown fresh.
  useEffect(() => {
    if (!o.expiresIn || !o.tokenAcquiredAt) return;
    const interval = setInterval(() => setTick((t) => t + 1), 30_000);
    return () => clearInterval(interval);
  }, [o.expiresIn, o.tokenAcquiredAt]);

  if (!o.accessToken && !o.idToken) return null;

  const expired = isExpired(o.expiresIn, o.tokenAcquiredAt);

  return (
    <div className='rounded-md border border-border/50 bg-muted/20'>
      {o.accessToken && (
        <div>
          <button
            type='button'
            className='flex w-full items-center justify-between px-3 py-2 text-sm hover:bg-muted/40 cursor-pointer'
            onClick={() => setAccessOpen(!accessOpen)}
          >
            <span className='flex items-center gap-2'>
              {accessOpen ? (
                <ChevronDown className='h-3.5 w-3.5' />
              ) : (
                <ChevronRight className='h-3.5 w-3.5' />
              )}
              <Key className='h-3.5 w-3.5 text-muted-foreground' />
              <span className='font-medium'>Access Token</span>
            </span>
            <span className={`text-2xs ${expired ? 'text-destructive' : 'text-muted-foreground'}`}>
              {tokenExpiryDisplay(o.expiresIn, o.tokenAcquiredAt)}
            </span>
          </button>
          {accessOpen && (
            <div className='px-3 pb-3'>
              <div className='flex gap-1.5'>
                <Input
                  className='flex-1 text-sm font-mono truncate'
                  readOnly
                  value={o.accessToken}
                />
                <Button
                  variant='outline'
                  size='sm'
                  className='px-2 text-sm shrink-0'
                  onClick={() => navigator.clipboard.writeText(o.accessToken)}
                  title='Copy'
                >
                  <Copy className='h-3 w-3' />
                </Button>
              </div>
            </div>
          )}
        </div>
      )}

      {o.idToken && (
        <div className={o.accessToken ? 'border-t border-border/30' : ''}>
          <button
            type='button'
            className='flex w-full items-center gap-2 px-3 py-2 text-sm hover:bg-muted/40 cursor-pointer'
            onClick={() => setIdOpen(!idOpen)}
          >
            {idOpen ? (
              <ChevronDown className='h-3.5 w-3.5' />
            ) : (
              <ChevronRight className='h-3.5 w-3.5' />
            )}
            <Key className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='font-medium'>ID Token</span>
          </button>
          {idOpen && (
            <div className='px-3 pb-3 space-y-1.5'>
              {o.idTokenClaims ? (
                <>
                  {o.idTokenClaims.subject && (
                    <div className='flex text-xs'>
                      <span className='w-20 text-muted-foreground'>Subject</span>
                      <span className='font-mono'>{o.idTokenClaims.subject}</span>
                    </div>
                  )}
                  {o.idTokenClaims.issuer && (
                    <div className='flex text-xs'>
                      <span className='w-20 text-muted-foreground'>Issuer</span>
                      <span className='font-mono truncate'>{o.idTokenClaims.issuer}</span>
                    </div>
                  )}
                  {o.idTokenClaims.audience && (
                    <div className='flex text-xs'>
                      <span className='w-20 text-muted-foreground'>Audience</span>
                      <span className='font-mono'>{o.idTokenClaims.audience}</span>
                    </div>
                  )}
                  {o.idTokenClaims.expiry && (
                    <div className='flex text-xs'>
                      <span className='w-20 text-muted-foreground'>Expires</span>
                      <span>{formatTimestamp(o.idTokenClaims.expiry)}</span>
                    </div>
                  )}
                  {o.idTokenClaims.issuedAt && (
                    <div className='flex text-xs'>
                      <span className='w-20 text-muted-foreground'>Issued</span>
                      <span>{formatTimestamp(o.idTokenClaims.issuedAt)}</span>
                    </div>
                  )}
                  {o.idTokenClaims.algorithm && (
                    <div className='flex text-xs'>
                      <span className='w-20 text-muted-foreground'>Algorithm</span>
                      <span>{o.idTokenClaims.algorithm}</span>
                    </div>
                  )}
                  <Button
                    variant='ghost'
                    size='sm'
                    className='text-xs mt-1'
                    onClick={() => setShowRawPayload(!showRawPayload)}
                  >
                    {showRawPayload ? 'Hide' : 'View'} Raw Payload
                  </Button>
                  {showRawPayload && (
                    <pre className='text-xs font-mono bg-muted p-2 rounded max-h-40 overflow-auto whitespace-pre-wrap'>
                      {o.idTokenClaims.rawPayload}
                    </pre>
                  )}
                </>
              ) : (
                <div className='flex gap-1.5'>
                  <Input className='flex-1 text-sm font-mono truncate' readOnly value={o.idToken} />
                  <Button
                    variant='outline'
                    size='sm'
                    className='px-2 text-sm shrink-0'
                    onClick={() => navigator.clipboard.writeText(o.idToken)}
                    title='Copy'
                  >
                    <Copy className='h-3 w-3' />
                  </Button>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {(o.tokenType || o.responseScope) && (
        <div className='flex gap-4 px-3 py-1.5 text-2xs text-muted-foreground border-t border-border/30'>
          {o.tokenType && <span>Token Type: {o.tokenType}</span>}
          {o.responseScope && <span>Scope: {o.responseScope}</span>}
        </div>
      )}
    </div>
  );
}
