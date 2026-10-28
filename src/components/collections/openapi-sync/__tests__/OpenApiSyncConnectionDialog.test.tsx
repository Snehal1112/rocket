import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearApplyState } from '@/lib/queries/openapi-sync-queries';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncConnectionDialog } from '../OpenApiSyncConnectionDialog';
import { makeStatus } from './fixtures';

const GH = 'https://github.com/acme/orders/blob/main/openapi.yaml';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    setOpenApiSyncInterval: vi.fn(),
    setOpenApiSyncToken: vi.fn(),
    clearOpenApiSyncToken: vi.fn(),
    revokeOpenApiSyncConsent: vi.fn(),
    grantOpenApiSyncConsent: vi.fn(),
    disconnectOpenApiSync: vi.fn(),
    previewOpenApiChangeLink: vi.fn(),
    confirmOpenApiChangeLink: vi.fn(),
    checkOpenApiSyncNow: vi.fn(),
  };
});

type Status = ReturnType<typeof makeStatus>;

function renderDialog(collection: string, status: Status, focus?: 'token') {
  renderWithQuery(
    <OpenApiSyncConnectionDialog
      collection={collection}
      status={status}
      open
      onOpenChange={vi.fn()}
      focus={focus}
    />,
  );
  return userEvent.setup();
}

const SWITCH = 'Check for spec updates automatically';

describe('OpenApiSyncConnectionDialog', () => {
  // jsdom lacks the pointer and scroll APIs the Radix Select relies on.
  beforeAll(() => {
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
    HTMLElement.prototype.setPointerCapture = vi.fn();
    HTMLElement.prototype.releasePointerCapture = vi.fn();
    HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    clearApplyState();
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasToken: false }));
  });
  afterEach(() => clearApplyState());

  it('has the title and asks for hasToken only while open', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const tree = (open: boolean) => (
      <QueryClientProvider client={queryClient}>
        <OpenApiSyncConnectionDialog
          collection='d0'
          status={makeStatus()}
          open={open}
          onOpenChange={vi.fn()}
        />
      </QueryClientProvider>
    );
    const view = render(tree(false));
    expect(tauriApi.getOpenApiSyncStatus).not.toHaveBeenCalled();
    view.rerender(tree(true));
    expect(await screen.findByText('Connection settings')).toBeInTheDocument();
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledWith('d0', true));
  });

  describe('check after turning checks on', () => {
    beforeEach(() => {
      vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
    });

    it('the_check_runs_when_checks_are_turned_on', async () => {
      const user = renderDialog('r1', makeStatus({ interval: 'off' }));
      await user.click(screen.getByRole('switch', { name: SWITCH }));
      await waitFor(() => expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('r1'));
      expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledTimes(1);
    });

    it('one_enable_runs_exactly_one_check_even_on_a_double_click', async () => {
      const user = renderDialog('r0', makeStatus({ interval: 'off' }));
      const toggle = screen.getByRole('switch', { name: SWITCH });
      await user.dblClick(toggle);
      await waitFor(() => expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledTimes(1));
      expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledTimes(1);
    });

    it('the_outcome_shows_in_the_status_row', async () => {
      const user = renderDialog('r2', makeStatus({ interval: 'off' }));
      await user.click(screen.getByRole('switch', { name: SWITCH }));
      const row = screen.getByRole('region', { name: 'Status' });
      expect(await within(row).findByText('No changes in the spec.')).toBeInTheDocument();
    });

    it('no_check_when_the_interval_only_changes', async () => {
      const user = renderDialog('r3', makeStatus({ interval: 'hourly' }));
      await user.click(screen.getByRole('combobox', { name: 'Check frequency' }));
      await user.click(screen.getByRole('option', { name: 'Every week' }));
      await waitFor(() => expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalled());
      expect(tauriApi.checkOpenApiSyncNow).not.toHaveBeenCalled();
    });

    it('no_check_when_turning_checks_off', async () => {
      const user = renderDialog('r4', makeStatus({ interval: 'hourly' }));
      await user.click(screen.getByRole('switch', { name: SWITCH }));
      await waitFor(() => expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('r4', 'off'));
      expect(tauriApi.checkOpenApiSyncNow).not.toHaveBeenCalled();
    });

    it('no_check_without_consent', async () => {
      const user = renderDialog('r5', makeStatus({ interval: 'off', consent: 'missing' }));
      await user.click(screen.getByRole('switch', { name: SWITCH }));
      expect(tauriApi.setOpenApiSyncInterval).not.toHaveBeenCalled();
      expect(tauriApi.checkOpenApiSyncNow).not.toHaveBeenCalled();
    });

    it('no_check_when_saving_the_interval_fails', async () => {
      vi.mocked(tauriApi.setOpenApiSyncInterval).mockRejectedValue('x');
      const user = renderDialog('r6', makeStatus({ interval: 'off' }));
      await user.click(screen.getByRole('switch', { name: SWITCH }));
      await waitFor(() => expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalled());
      expect(tauriApi.checkOpenApiSyncNow).not.toHaveBeenCalled();
    });
  });

  describe('status row', () => {
    it('the_status_row_shows_the_last_check', () => {
      renderDialog('s1', makeStatus());
      const row = screen.getByRole('region', { name: 'Status' });
      expect(row).toHaveTextContent('Last checked 2h ago. No problems.');
    });

    it('the_status_row_says_when_it_was_never_checked', () => {
      renderDialog('s2', makeStatus({ lastCheckedAt: null, lastStatus: null }));
      const row = screen.getByRole('region', { name: 'Status' });
      expect(row).toHaveTextContent('Not checked yet.');
      expect(row).not.toHaveTextContent('No problems');
    });

    it('the_status_row_does_not_claim_no_problems_without_approval', () => {
      renderDialog('s6', makeStatus({ consent: 'missing' }));
      const row = screen.getByRole('region', { name: 'Status' });
      expect(row).not.toHaveTextContent('No problems');
    });

    it('the_status_row_shows_the_failure_sentence', () => {
      renderDialog(
        's3',
        makeStatus({
          lastStatus: { state: 'failed', code: 'network' },
          lastMessage: 'The server could not be reached.',
        }),
      );
      const row = screen.getByRole('region', { name: 'Status' });
      expect(row).toHaveTextContent('The server could not be reached.');
      expect(row).not.toHaveTextContent('No problems');
    });

    it('check_now_in_the_dialog_runs_a_check', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
      const user = renderDialog('s4', makeStatus());
      const row = screen.getByRole('region', { name: 'Status' });
      await user.click(within(row).getByRole('button', { name: 'Check now' }));
      await waitFor(() => expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('s4'));
    });

    it('check_now_is_disabled_without_approval', () => {
      renderDialog('s5', makeStatus({ consent: 'missing' }));
      const row = screen.getByRole('region', { name: 'Status' });
      expect(within(row).getByRole('button', { name: 'Check now' })).toBeDisabled();
    });
  });

  describe('rate limit hint', () => {
    const HINT = /60 requests an hour without a token/;
    // Lets the token query resolve, so a hidden hint is not just a hint that has not loaded yet.
    const settled = () => act(async () => new Promise((r) => setTimeout(r, 50)));

    it('the_hint_shows_for_five_minutes_on_github_without_a_token', async () => {
      renderDialog('h1', makeStatus({ location: GH, interval: 'every_5_minutes' }));
      expect(await screen.findByText(HINT)).toBeInTheDocument();
    });

    it('no_rate_limit_hint_with_a_token', async () => {
      vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
        makeStatus({ location: GH, interval: 'every_5_minutes', hasToken: true }),
      );
      renderDialog('h2', makeStatus({ location: GH, interval: 'every_5_minutes' }));
      await screen.findByText('A token is stored');
      expect(screen.queryByText(HINT)).toBeNull();
    });

    it('no_rate_limit_hint_for_a_non_github_link', async () => {
      // The same status on a GitHub link shows the hint, so a missing hint here is the link's doing.
      renderDialog('h3', makeStatus({ interval: 'every_5_minutes' }));
      await settled();
      expect(screen.queryByText(HINT)).toBeNull();
    });

    it('no_rate_limit_hint_for_a_longer_interval', async () => {
      renderDialog('h4', makeStatus({ location: GH, interval: 'hourly' }));
      await settled();
      expect(screen.queryByText(HINT)).toBeNull();
    });

    it('no_rate_limit_hint_without_approval', async () => {
      renderDialog(
        'h6',
        makeStatus({ location: GH, interval: 'every_5_minutes', consent: 'missing' }),
      );
      await settled();
      expect(screen.queryByText(HINT)).toBeNull();
    });

    it('no_rate_limit_hint_while_the_token_flag_loads', async () => {
      vi.mocked(tauriApi.getOpenApiSyncStatus).mockReturnValue(new Promise(() => undefined));
      renderDialog('h7', makeStatus({ location: GH, interval: 'every_5_minutes' }));
      await settled();
      expect(screen.queryByText(HINT)).toBeNull();
    });

    it('add_token_in_the_hint_focuses_the_token_field', async () => {
      const user = renderDialog('h5', makeStatus({ location: GH, interval: 'every_5_minutes' }));
      await screen.findByText(HINT);
      await user.click(screen.getByRole('button', { name: 'Add token' }));
      expect(screen.getByLabelText('Access token')).toHaveFocus();
    });
  });

  it('focus_token_scrolls_to_the_access_section', async () => {
    const github = makeStatus({ location: 'https://github.com/acme/api/blob/main/o.yaml' });
    renderDialog('d30', github, 'token');
    const input = await screen.findByLabelText('Access token');
    await waitFor(() => expect(input).toHaveFocus());
    expect(screen.getByRole('switch', { name: SWITCH })).toBeInTheDocument();
  });

  it('without_focus_the_token_field_is_not_focused', async () => {
    const github = makeStatus({ location: 'https://github.com/acme/api/blob/main/o.yaml' });
    renderDialog('d31', github);
    const input = await screen.findByLabelText('Access token');
    expect(input).not.toHaveFocus();
  });

  it('turning the switch on sets 5 minutes', async () => {
    vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
    const user = renderDialog('d1', makeStatus({ interval: 'off' }));
    await user.click(screen.getByRole('switch', { name: SWITCH }));
    expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('d1', 'every_5_minutes');
  });

  it('turning the switch off sets off', async () => {
    vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
    const user = renderDialog('d2', makeStatus({ interval: 'hourly' }));
    const sw = screen.getByRole('switch', { name: SWITCH });
    expect(sw).toBeChecked();
    await user.click(sw);
    expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('d2', 'off');
  });

  it('shows the consent banner and changes nothing when consent is missing', async () => {
    const user = renderDialog('d4', makeStatus({ consent: 'missing', interval: 'off' }));
    await user.click(screen.getByRole('switch', { name: SWITCH }));
    expect(tauriApi.setOpenApiSyncInterval).not.toHaveBeenCalled();
    expect(await screen.findByText('Trust this spec link')).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Check frequency' })).toBeDisabled();
  });

  it('turns back on with the last frequency the stored status had', () => {
    vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const tree = (interval: 'off' | 'weekly') => (
      <QueryClientProvider client={queryClient}>
        <OpenApiSyncConnectionDialog
          collection='d18'
          status={makeStatus({ interval })}
          open
          onOpenChange={vi.fn()}
        />
      </QueryClientProvider>
    );
    const view = render(tree('weekly'));
    view.rerender(tree('off'));
    const user = userEvent.setup();
    return user.click(screen.getByRole('switch', { name: SWITCH })).then(() => {
      expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('d18', 'weekly');
    });
  });

  it('lists the 7 frequencies and sets one', async () => {
    vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
    const user = renderDialog('d5', makeStatus({ interval: 'daily' }));
    await user.click(screen.getByRole('combobox', { name: 'Check frequency' }));
    const options = await screen.findAllByRole('option');
    expect(options.map((o) => o.textContent)).toEqual([
      'Every 5 minutes',
      'Every 15 minutes',
      'Every 30 minutes',
      'Every hour',
      'Every day',
      'Every week',
      'Every month',
    ]);
    await user.click(screen.getByRole('option', { name: 'Every week' }));
    expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('d5', 'weekly');
  });

  it('disables the frequency while the switch is off', () => {
    renderDialog('d6', makeStatus({ interval: 'off' }));
    expect(screen.getByRole('combobox', { name: 'Check frequency' })).toBeDisabled();
  });

  it('says so and shows an empty select for an interval it does not know', () => {
    renderDialog('d7', makeStatus({ interval: 'off', intervalUnknown: true }));
    expect(
      screen.getByText('This setting is not understood. Pick a frequency.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: SWITCH })).not.toBeChecked();
    const select = screen.getByRole('combobox', { name: 'Check frequency' });
    expect(select).toBeEnabled();
    expect(select).toHaveAttribute('aria-describedby', 'openapi-sync-unknown');
    expect(select).toHaveTextContent('');
  });

  it('shows the location with the query behind Show full link', async () => {
    const user = renderDialog(
      'd8',
      makeStatus({ location: 'https://api.example.com/v1/spec.json?token=abc' }),
    );
    expect(screen.getByText('api.example.com/v1/spec.json')).toBeInTheDocument();
    expect(screen.queryByText(/token=abc/)).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText('https://api.example.com/v1/spec.json?token=abc')).toBeInTheDocument();
  });

  it('keeps the token field write-only, empty and cleared after save', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockResolvedValue(undefined);
    const user = renderDialog('d9', makeStatus({ location: GH }));
    const input = await screen.findByLabelText('Access token');
    expect(input).toHaveAttribute('type', 'password');
    expect(input).toHaveAttribute('autocomplete', 'new-password');
    expect(input).toHaveValue('');
    await user.type(input, 'ghp_secret');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    expect(tauriApi.setOpenApiSyncToken).toHaveBeenCalledWith('d9', 'ghp_secret');
    await waitFor(() => expect(screen.getByLabelText('Access token')).toHaveValue(''));
  });

  it('shows the consent sentence when saving without approval', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockRejectedValue('consent_required: no grant');
    const user = renderDialog('d10', makeStatus({ consent: 'missing', location: GH }));
    await user.type(await screen.findByLabelText('Access token'), 'tok');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    expect(
      await screen.findByText(/Approve this spec link on this machine first/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/consent_required/)).toBeNull();
    expect(screen.getByLabelText('Access token')).toHaveValue('');
  });

  it('shows that a token is stored and clears it', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasToken: true }));
    vi.mocked(tauriApi.clearOpenApiSyncToken).mockResolvedValue(undefined);
    const user = renderDialog('d11', makeStatus({ location: GH }));
    expect(await screen.findByText('A token is stored')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Clear' }));
    expect(tauriApi.clearOpenApiSyncToken).toHaveBeenCalledWith('d11');
  });

  it('leaves no token in the mutation cache after the save settles', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockResolvedValue(undefined);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <OpenApiSyncConnectionDialog
          collection='d12'
          status={makeStatus({ location: GH })}
          open
          onOpenChange={vi.fn()}
        />
      </QueryClientProvider>,
    );
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Access token'), 'ghp_secret');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    await waitFor(() => expect(tauriApi.setOpenApiSyncToken).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByLabelText('Access token')).toHaveValue(''));
    await waitFor(() =>
      expect(
        JSON.stringify(
          queryClient
            .getMutationCache()
            .getAll()
            .map((m) => m.state),
        ),
      ).not.toContain('ghp_secret'),
    );
  });

  it('hides Clear when no token is stored', async () => {
    renderDialog('d13', makeStatus({ location: GH }));
    await screen.findByLabelText('Access token');
    expect(screen.queryByRole('button', { name: 'Clear' })).toBeNull();
  });

  it('hides the token field for a link that is not on GitHub', () => {
    renderDialog('d14', makeStatus());
    expect(screen.getByText('Tokens work with GitHub links only.')).toBeInTheDocument();
    expect(screen.queryByLabelText('Access token')).toBeNull();
  });

  it('shows_that_the_link_is_approved_on_this_machine', () => {
    renderDialog('d30', makeStatus({ consent: 'granted' }));
    expect(screen.getByText('Approved on this machine')).toBeInTheDocument();
    expect(screen.queryByText('Not approved yet')).toBeNull();
  });

  it('shows_that_the_link_is_not_approved_yet', () => {
    renderDialog('d31', makeStatus({ consent: 'missing' }));
    expect(screen.getByText('Not approved yet')).toBeInTheDocument();
    expect(screen.queryByText('Approved on this machine')).toBeNull();
  });

  it('says_what_stop_trusting_and_disconnect_each_do', () => {
    renderDialog('d32', makeStatus());
    expect(
      screen.getByText(
        'Stops background checks and token use until you approve the link again.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Removes the sync link from this collection. Your requests stay.'),
    ).toBeInTheDocument();
  });

  it('stops trusting the link', async () => {
    vi.mocked(tauriApi.revokeOpenApiSyncConsent).mockResolvedValue(undefined);
    const user = renderDialog('d15', makeStatus());
    await user.click(screen.getByRole('button', { name: 'Stop trusting this link' }));
    expect(tauriApi.revokeOpenApiSyncConsent).toHaveBeenCalledWith('d15');
  });

  it('hides Stop trusting after approval is gone', () => {
    renderDialog('d16', makeStatus({ consent: 'missing' }));
    expect(screen.queryByRole('button', { name: 'Stop trusting this link' })).toBeNull();
  });

  it('opens the disconnect confirmation from the Disconnect button', async () => {
    const user = renderDialog('d17', makeStatus());
    await user.click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(
      await screen.findByText('Rocket stops checking this spec. Your requests are not changed.'),
    ).toBeInTheDocument();
  });
  it('the_dialog_shows_change_link_in_the_spec_link_section', async () => {
    const user = renderDialog('Orders', makeStatus());
    const section = screen.getByRole('region', { name: 'Spec link' });
    await user.click(within(section).getByRole('button', { name: 'Change link' }));
    expect(screen.getByLabelText('New spec link')).toBeInTheDocument();
    // The toggle goes away while the form is open, so only one Change link button can show.
    expect(screen.queryByRole('button', { name: 'Change link' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByLabelText('New spec link')).not.toBeInTheDocument();
    expect(within(section).getByRole('button', { name: 'Change link' })).toBeInTheDocument();
  });

  it('cannot_close_while_confirming', async () => {
    vi.mocked(tauriApi.previewOpenApiChangeLink).mockResolvedValue({
      specSha256: 's',
      title: 'Orders',
      version: null,
      matched: 2,
      added: 0,
      removed: 0,
      sameSpec: false,
    });
    vi.mocked(tauriApi.confirmOpenApiChangeLink).mockReturnValue(new Promise(() => {}));
    const onOpenChange = vi.fn();
    renderWithQuery(
      <OpenApiSyncConnectionDialog
        collection='Orders'
        status={makeStatus()}
        open
        onOpenChange={onOpenChange}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Change link' }));
    await user.type(screen.getByLabelText('New spec link'), 'https://example.com/other.json');
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    await user.click(await screen.findByRole('button', { name: 'Change link' }));
    await user.keyboard('{Escape}');
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });
});
