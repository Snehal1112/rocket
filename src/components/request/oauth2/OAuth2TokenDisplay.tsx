import { Check, ChevronDown, ChevronRight, Copy, Key, ShieldCheck } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
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
  return `Expires in ${Math.floor(remaining / 3600)}h ${Math.floor((remaining % 3600) / 60)}m (at ${time})`;
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
  // Start expanded whenever a token is present. User can collapse manually.
  const [accessOpen, setAccessOpen] = useState(!!o.accessToken);
  const [idOpen, setIdOpen] = useState(!!o.idToken);
  const [showRawAccessPayload, setShowRawAccessPayload] = useState(false);
  const [showRawPayload, setShowRawPayload] = useState(false);
  const [, setTick] = useState(0);
  const [copied, setCopied] = useState<'access' | 'id' | null>(null);

  const copy = (text: string, which: 'access' | 'id') => {
    navigator.clipboard.writeText(text);
    setCopied(which);
    setTimeout(() => setCopied(null), 1500);
  };

  // Track previous token values so we only auto-expand on a NEW token arriving,
  // not on every re-render. This way a manual collapse stays sticky until the
  // next fetch replaces the token.
  const prevAccessToken = useRef(o.accessToken);
  const prevIdToken = useRef(o.idToken);

  // Re-render every 30s to keep the expiry countdown fresh.
  useEffect(() => {
    if (!o.expiresIn || !o.tokenAcquiredAt) return;
    const interval = setInterval(() => setTick((t) => t + 1), 30_000);
    return () => clearInterval(interval);
  }, [o.expiresIn, o.tokenAcquiredAt]);

  // Auto-expand when a new (different) access token arrives; auto-close when cleared.
  useEffect(() => {
    if (o.accessToken !== prevAccessToken.current) {
      prevAccessToken.current = o.accessToken;
      setAccessOpen(!!o.accessToken);
    }
  }, [o.accessToken]);

  // Auto-expand when a new id token arrives; auto-close when cleared.
  useEffect(() => {
    if (o.idToken !== prevIdToken.current) {
      prevIdToken.current = o.idToken;
      setIdOpen(!!o.idToken);
    }
  }, [o.idToken]);

  if (!o.accessToken && !o.idToken) return null;

  const expired = isExpired(o.expiresIn, o.tokenAcquiredAt);

  return (
    <div className='rounded-md border border-border/60 bg-muted/15 overflow-hidden'>
      {o.accessToken && (
        <div>
          <button
            type='button'
            className='flex w-full items-center justify-between px-3 py-2.5 text-sm hover:bg-muted/40 focus-visible:outline-2 focus-visible:outline-ring rounded-t-md transition-colors'
            onClick={() => setAccessOpen((v) => !v)}
            aria-expanded={accessOpen}
          >
            <span className='flex items-center gap-2'>
              {accessOpen ? (
                <ChevronDown className='h-3.5 w-3.5 text-foreground/60' aria-hidden='true' />
              ) : (
                <ChevronRight className='h-3.5 w-3.5 text-foreground/60' aria-hidden='true' />
              )}
              <Key className='h-3.5 w-3.5 text-foreground/50' aria-hidden='true' />
              <span className='font-medium text-foreground/90'>Access Token</span>
            </span>
            <span
              className={`rounded px-2 py-0.5 text-xs font-medium ${expired ? 'bg-destructive/15 text-destructive' : 'text-muted-foreground'}`}
            >
              {tokenExpiryDisplay(o.expiresIn, o.tokenAcquiredAt)}
            </span>
          </button>
          {accessOpen && (
            <div className='px-3 pb-3 pt-1 space-y-2'>
              <div className='flex gap-2 items-start'>
                <Textarea
                  className='flex-1 text-xs font-mono resize-none min-h-[4.5rem] max-h-40'
                  readOnly
                  value={o.accessToken}
                  aria-label='Access token value'
                />
                <Button
                  variant='outline'
                  size='sm'
                  className='px-2.5 shrink-0 min-h-[36px]'
                  onClick={() => copy(o.accessToken, 'access')}
                  aria-label={copied === 'access' ? 'Copied!' : 'Copy access token'}
                >
                  {copied === 'access' ? (
                    <Check className='h-3.5 w-3.5 text-green-500' aria-hidden='true' />
                  ) : (
                    <Copy className='h-3.5 w-3.5' aria-hidden='true' />
                  )}
                </Button>
              </div>
              {o.accessTokenClaims && (
                <div className='rounded border border-border/40 bg-muted/20 p-2.5 space-y-2'>
                  <div className='flex items-center gap-1.5 text-xs font-semibold uppercase tracking-[0.06em] text-foreground/50'>
                    <ShieldCheck className='h-3 w-3' aria-hidden='true' />
                    Decoded Payload
                  </div>
                  <dl className='space-y-1.5'>
                    {o.accessTokenClaims.subject && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Subject</dt>
                        <dd className='font-mono truncate min-w-0'>
                          {o.accessTokenClaims.subject}
                        </dd>
                      </div>
                    )}
                    {o.accessTokenClaims.issuer && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Issuer</dt>
                        <dd className='font-mono truncate min-w-0'>{o.accessTokenClaims.issuer}</dd>
                      </div>
                    )}
                    {o.accessTokenClaims.audience && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>
                          Audience
                        </dt>
                        <dd className='font-mono truncate min-w-0'>
                          {o.accessTokenClaims.audience}
                        </dd>
                      </div>
                    )}
                    {o.accessTokenClaims.expiry && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Expires</dt>
                        <dd>{formatTimestamp(o.accessTokenClaims.expiry)}</dd>
                      </div>
                    )}
                    {o.accessTokenClaims.issuedAt && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Issued</dt>
                        <dd>{formatTimestamp(o.accessTokenClaims.issuedAt)}</dd>
                      </div>
                    )}
                    {o.accessTokenClaims.scope && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Scope</dt>
                        <dd className='font-mono break-all min-w-0'>{o.accessTokenClaims.scope}</dd>
                      </div>
                    )}
                    {o.accessTokenClaims.algorithm && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>
                          Algorithm
                        </dt>
                        <dd>{o.accessTokenClaims.algorithm}</dd>
                      </div>
                    )}
                  </dl>
                  <Button
                    variant='ghost'
                    size='sm'
                    className='text-sm mt-1 h-8 px-2'
                    onClick={() => setShowRawAccessPayload((v) => !v)}
                    aria-expanded={showRawAccessPayload}
                  >
                    {showRawAccessPayload ? 'Hide' : 'View'} Raw Payload
                  </Button>
                  {showRawAccessPayload && (
                    <pre className='text-xs font-mono bg-muted/60 border border-border/40 p-2.5 rounded max-h-40 overflow-auto whitespace-pre-wrap'>
                      {o.accessTokenClaims.rawPayload}
                    </pre>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {o.idToken && (
        <div className={o.accessToken ? 'border-t border-border/40' : ''}>
          <button
            type='button'
            className='flex w-full items-center gap-2 px-3 py-2.5 text-sm hover:bg-muted/40 focus-visible:outline-2 focus-visible:outline-ring transition-colors'
            onClick={() => setIdOpen((v) => !v)}
            aria-expanded={idOpen}
          >
            {idOpen ? (
              <ChevronDown className='h-3.5 w-3.5 text-foreground/60' aria-hidden='true' />
            ) : (
              <ChevronRight className='h-3.5 w-3.5 text-foreground/60' aria-hidden='true' />
            )}
            <Key className='h-3.5 w-3.5 text-foreground/50' aria-hidden='true' />
            <span className='font-medium text-foreground/90'>ID Token</span>
          </button>
          {idOpen && (
            <div className='px-3 pb-3 pt-1 space-y-2'>
              {o.idTokenClaims ? (
                <>
                  <dl className='space-y-1.5'>
                    {o.idTokenClaims.subject && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Subject</dt>
                        <dd className='font-mono truncate min-w-0'>{o.idTokenClaims.subject}</dd>
                      </div>
                    )}
                    {o.idTokenClaims.issuer && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Issuer</dt>
                        <dd className='font-mono truncate min-w-0'>{o.idTokenClaims.issuer}</dd>
                      </div>
                    )}
                    {o.idTokenClaims.audience && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>
                          Audience
                        </dt>
                        <dd className='font-mono truncate min-w-0'>{o.idTokenClaims.audience}</dd>
                      </div>
                    )}
                    {o.idTokenClaims.expiry && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Expires</dt>
                        <dd>{formatTimestamp(o.idTokenClaims.expiry)}</dd>
                      </div>
                    )}
                    {o.idTokenClaims.issuedAt && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>Issued</dt>
                        <dd>{formatTimestamp(o.idTokenClaims.issuedAt)}</dd>
                      </div>
                    )}
                    {o.idTokenClaims.algorithm && (
                      <div className='flex gap-3 text-sm'>
                        <dt className='w-20 shrink-0 text-muted-foreground font-medium'>
                          Algorithm
                        </dt>
                        <dd>{o.idTokenClaims.algorithm}</dd>
                      </div>
                    )}
                  </dl>
                  <Button
                    variant='ghost'
                    size='sm'
                    className='text-sm mt-1 h-8 px-2'
                    onClick={() => setShowRawPayload((v) => !v)}
                    aria-expanded={showRawPayload}
                  >
                    {showRawPayload ? 'Hide' : 'View'} Raw Payload
                  </Button>
                  {showRawPayload && (
                    <pre className='text-xs font-mono bg-muted/60 border border-border/40 p-2.5 rounded max-h-40 overflow-auto whitespace-pre-wrap'>
                      {o.idTokenClaims.rawPayload}
                    </pre>
                  )}
                </>
              ) : (
                <div className='flex gap-2 items-center'>
                  <Input
                    className='flex-1 text-sm font-mono'
                    readOnly
                    value={o.idToken}
                    aria-label='ID token value'
                  />
                  <Button
                    variant='outline'
                    size='sm'
                    className='px-2.5 shrink-0 min-h-[36px]'
                    onClick={() => copy(o.idToken, 'id')}
                    aria-label={copied === 'id' ? 'Copied!' : 'Copy ID token'}
                  >
                    {copied === 'id' ? (
                      <Check className='h-3.5 w-3.5 text-green-500' aria-hidden='true' />
                    ) : (
                      <Copy className='h-3.5 w-3.5' aria-hidden='true' />
                    )}
                  </Button>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {(o.tokenType || o.responseScope) && (
        <div className='flex flex-wrap gap-4 px-3 py-2 text-xs text-muted-foreground border-t border-border/40 bg-muted/10'>
          {o.tokenType && (
            <span>
              <span className='font-medium text-foreground/60'>Type:</span> {o.tokenType}
            </span>
          )}
          {o.responseScope && (
            <span>
              <span className='font-medium text-foreground/60'>Scope:</span> {o.responseScope}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
