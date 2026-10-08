import { KeyRound, Loader2, LogIn } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { type Oauth2TokenStatus, oauth2TokenStatus } from '@/lib/flow-auth';
import {
  type AuthenticateResult,
  type AuthNode,
  type AuthNodeScope,
  authenticateAuthNode,
} from '@/lib/flow-auth-preflight';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface Message {
  kind: 'status' | 'error';
  text: string;
}

function describeStatus(status: Oauth2TokenStatus): string {
  switch (status.kind) {
    case 'none':
      return 'No token';
    case 'expired':
      return 'Token expired';
    case 'valid': {
      if (status.expiresAt === null) return 'Token valid';
      const time = new Date(status.expiresAt * 1000).toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
      });
      return `Token valid until ${time}`;
    }
  }
}

function messageFor(result: AuthenticateResult): Message {
  switch (result.source) {
    case 'discarded':
      return {
        kind: 'status',
        text: 'The sign-in settings changed, so the new token was not saved.',
      };
    case 'backend':
      return { kind: 'status', text: 'This grant is fetched when the flow runs.' };
    case 'stored':
      return { kind: 'status', text: 'Already signed in.' };
    case 'refreshed':
      return { kind: 'status', text: 'Token refreshed.' };
    case 'signed-in':
      return result.accessToken
        ? { kind: 'status', text: 'Signed in.' }
        : { kind: 'error', text: 'The provider returned no token.' };
  }
}

/**
 * Signs in an interactive OAuth 2.0 Auth node without starting a run, and shows
 * whether the node holds a valid token. The token stays in the in-memory store;
 * this component never renders it.
 */
export function AuthenticateButton({
  node,
  scope,
  oauth,
}: {
  node: AuthNode;
  scope: AuthNodeScope;
  oauth: OAuth2State | undefined;
}) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);
  const busyRef = useRef(false);
  const mountedRef = useRef(true);
  // The node's persisted auth as of the latest render. A sign-in compares it with
  // the value at click time, to notice an edit made while the window was open.
  const latestAuthRef = useRef('');
  latestAuthRef.current = JSON.stringify(node.kind.auth);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const status = oauth2TokenStatus(oauth);
  const signedIn = status.kind === 'valid';

  const run = async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setMessage(null);
    const authAtClick = latestAuthRef.current;
    try {
      const result = await authenticateAuthNode(scope, node, {
        force: signedIn,
        shouldWrite: () => latestAuthRef.current === authAtClick,
      });
      if (mountedRef.current) setMessage(messageFor(result));
    } catch (err) {
      if (mountedRef.current) {
        setMessage({ kind: 'error', text: err instanceof Error ? err.message : String(err) });
      }
    } finally {
      busyRef.current = false;
      if (mountedRef.current) setBusy(false);
    }
  };

  const Icon = signedIn ? KeyRound : LogIn;
  return (
    <div data-testid='authenticate-section' className='space-y-1.5'>
      <div className='flex items-center gap-2'>
        <Button
          type='button'
          size='sm'
          variant={signedIn ? 'outline' : 'default'}
          className='h-8 gap-1.5 text-xs'
          disabled={busy}
          aria-busy={busy}
          onClick={() => void run()}
        >
          {busy ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' aria-hidden='true' />
          ) : (
            <Icon className='h-3.5 w-3.5' aria-hidden='true' />
          )}
          {busy ? 'Signing in…' : signedIn ? 'Authenticate again' : 'Authenticate'}
        </Button>
        <Badge variant='secondary' className='text-xs font-normal'>
          {describeStatus(status)}
        </Badge>
      </div>
      {message?.kind === 'error' && (
        <p role='alert' className='break-words text-xs text-red-600'>
          {message.text}
        </p>
      )}
      {message?.kind === 'status' && (
        <p role='status' className='text-xs text-muted-foreground'>
          {message.text}
        </p>
      )}
      <p className='text-xs text-muted-foreground'>
        Opens the provider's sign-in page. The token stays in memory and is never saved to the flow
        file.
      </p>
    </div>
  );
}
