import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AuthenticateResult, AuthNode } from '@/lib/flow-auth-preflight';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import type { Auth } from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import type { AuthState } from '@/types/pane-types';
import { AuthenticateButton } from '../AuthenticateButton';

const preflight = vi.hoisted(() => ({ authenticateAuthNode: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => preflight);

type OAuth2State = NonNullable<AuthState['oauth2']>;
const TOKEN = 'secret-access-token-123456';

const oauthAuth = (scope = ''): Auth =>
  ({
    authType: 'o-auth2',
    flow: 'authorization_code',
    accessTokenUrl: 'https://idp/token',
    authorizationUrl: 'https://idp/authorize',
    credentials: { clientId: 'cid', clientSecret: '{{secret}}' },
    scope,
  }) as unknown as Auth;

const authNode = (auth: Auth = oauthAuth()): AuthNode => ({
  id: 'a1',
  position: { x: 0, y: 0 },
  kind: { kind: 'Auth', label: 'Sign in', auth, applyToInherit: true },
});

const scope = {
  collection: 'api',
  flowName: 'login',
  environmentName: 'dev',
  globalEnvName: 'global',
};

const oauth = (patch: Partial<OAuth2State> = {}): OAuth2State => ({
  ...(fromPersistedAuth(oauthAuth()).oauth2 as OAuth2State),
  ...patch,
});
const now = () => Math.floor(Date.now() / 1000);

const ui = (node = authNode(), state: OAuth2State | undefined = oauth()) => (
  <AuthenticateButton node={node} scope={scope} oauth={state} />
);

describe('AuthenticateButton', () => {
  beforeEach(() => {
    // Block body: returning the mock would make vitest call it as a teardown.
    preflight.authenticateAuthNode.mockReset();
  });

  it('offers Authenticate and says there is no token', () => {
    render(ui());
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeEnabled();
    expect(screen.getByText('No token')).toBeInTheDocument();
  });

  it('shows the expiry time of a valid token and offers to authenticate again', () => {
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 3600, tokenAcquiredAt: now() })));
    expect(screen.getByText(/^Token valid until \d{1,2}:\d{2}/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Authenticate again' })).toBeInTheDocument();
  });

  it('shows a valid token without a lifetime as valid', () => {
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: null })));
    expect(screen.getByText('Token valid')).toBeInTheDocument();
  });

  it('shows an expired token and offers a plain Authenticate', () => {
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 60, tokenAcquiredAt: 1 })));
    expect(screen.getByText('Token expired')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeInTheDocument();
  });

  it('signs in once, shows busy state, and ignores clicks while it is pending', async () => {
    const pending = createDeferred<AuthenticateResult>();
    preflight.authenticateAuthNode.mockReturnValue(pending.promise);
    render(ui());

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    const busy = screen.getByRole('button', { name: 'Signing in…' });
    expect(busy).toBeDisabled();
    expect(busy).toHaveAttribute('aria-busy', 'true');
    await userEvent.click(busy);

    expect(preflight.authenticateAuthNode).toHaveBeenCalledTimes(1);
    expect(preflight.authenticateAuthNode).toHaveBeenCalledWith(
      scope,
      expect.objectContaining({ id: 'a1' }),
      { force: false, shouldWrite: expect.any(Function) },
    );

    await act(async () => pending.resolve({ accessToken: TOKEN, source: 'signed-in' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Signed in.');
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeEnabled();
  });

  it('forces a new sign-in when a valid token is held', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ accessToken: TOKEN, source: 'signed-in' });
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 3600, tokenAcquiredAt: now() })));

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate again' }));

    expect(preflight.authenticateAuthNode).toHaveBeenCalledWith(
      scope,
      expect.anything(),
      expect.objectContaining({ force: true }),
    );
  });

  it('reports a refresh', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ accessToken: TOKEN, source: 'refreshed' });
    render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Token refreshed.');
  });

  it('shows a failed sign-in as an alert and lets the user try again', async () => {
    preflight.authenticateAuthNode.mockRejectedValue(
      new Error('Sign-in for Auth node "Sign in" failed: window closed'),
    );
    render(ui());

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Sign-in for Auth node "Sign in" failed: window closed',
    );
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeEnabled();
  });

  it('says so when the provider returns no token', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ source: 'signed-in' });
    render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The provider returned no token.');
  });

  it('only lets the token be written while the node configuration is unchanged', async () => {
    preflight.authenticateAuthNode.mockReturnValue(new Promise(() => undefined));
    const { rerender } = render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    const options = preflight.authenticateAuthNode.mock.calls[0][2] as {
      shouldWrite: () => boolean;
    };

    expect(options.shouldWrite()).toBe(true);
    rerender(ui(authNode(oauthAuth('changed-scope'))));
    expect(options.shouldWrite()).toBe(false);
  });

  it('tells the user when the new token was not saved', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ source: 'discarded' });
    render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      'The sign-in settings changed, so the new token was not saved.',
    );
  });

  it('never puts the token in the DOM or in an attribute', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ accessToken: TOKEN, source: 'signed-in' });
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 3600, tokenAcquiredAt: now() })));
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate again' }));
    await screen.findByRole('status');
    expect(screen.getByTestId('authenticate-section').outerHTML).not.toContain(TOKEN);
  });

  it('ignores a result that arrives after the component unmounted', async () => {
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const pending = createDeferred<AuthenticateResult>();
    preflight.authenticateAuthNode.mockReturnValue(pending.promise);
    const { unmount } = render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    unmount();
    await act(async () => pending.resolve({ accessToken: TOKEN, source: 'signed-in' }));

    expect(errors).not.toHaveBeenCalled();
    errors.mockRestore();
  });
});
