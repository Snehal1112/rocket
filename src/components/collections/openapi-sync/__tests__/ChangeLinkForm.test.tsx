import { screen, waitFor } from '@testing-library/react';
import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { ChangeLinkForm } from '../ChangeLinkForm';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    previewOpenApiChangeLink: vi.fn(),
    confirmOpenApiChangeLink: vi.fn(),
  };
});

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

vi.mock('sonner', () => ({ toast: { success: vi.fn(), info: vi.fn() } }));

const NEW_LINK = 'https://example.com/other.json';
const GH_LINK = 'https://github.com/acme/orders/blob/main/openapi.yaml';

function preview(over: Partial<tauriApi.OpenApiChangeLinkPreview> = {}) {
  return {
    specSha256: 'sha-1',
    title: 'Orders',
    version: '2',
    matched: 2,
    added: 1,
    removed: 1,
    sameSpec: false,
    ...over,
  };
}

function renderForm(props: { onDone?: () => void; onBusyChange?: (b: boolean) => void } = {}) {
  renderWithQuery(
    <ChangeLinkForm
      collection='Orders'
      currentLocation='https://example.com/openapi.json'
      onDone={props.onDone ?? vi.fn()}
      onBusyChange={props.onBusyChange}
    />,
  );
  return userEvent.setup();
}

async function check(user: ReturnType<typeof userEvent.setup>, link = NEW_LINK) {
  await user.type(screen.getByLabelText('New spec link'), link);
  await user.click(screen.getByRole('button', { name: 'Check link' }));
}

describe('ChangeLinkForm', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.previewOpenApiChangeLink).mockResolvedValue(preview());
    vi.mocked(tauriApi.confirmOpenApiChangeLink).mockResolvedValue({
      pending: true,
      tokenKept: false,
      warnings: [],
    });
  });

  it('checking_a_link_shows_the_match_counts', async () => {
    const user = renderForm();
    await check(user);
    expect(
      await screen.findByText(
        '2 of 3 endpoints match. A new spec will be reviewed in Spec updates.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Only \d+ of/)).not.toBeInTheDocument();
    expect(tauriApi.previewOpenApiChangeLink).toHaveBeenCalledWith('Orders', NEW_LINK, undefined);
  });

  it('change_link_previews_a_chosen_file', async () => {
    vi.mocked(openFilePicker).mockResolvedValue('/tmp/other.yaml');
    const user = renderForm();
    await user.click(screen.getByRole('button', { name: 'File' }));
    await user.click(screen.getByRole('button', { name: /choose spec file/i }));
    await user.click(await screen.findByRole('button', { name: 'Check link' }));
    await waitFor(() =>
      expect(tauriApi.previewOpenApiChangeLink).toHaveBeenCalledWith(
        'Orders',
        '/tmp/other.yaml',
        undefined,
      ),
    );
    expect(screen.queryByText('Access token (optional)')).toBeNull();
  });

  it('a_low_match_shows_the_warning', async () => {
    vi.mocked(tauriApi.previewOpenApiChangeLink).mockResolvedValue(
      preview({ matched: 1, removed: 4 }),
    );
    const user = renderForm();
    await check(user);
    expect(
      await screen.findByText(
        'Only 1 of 5 endpoints match this spec. Check that this is the right link.',
      ),
    ).toBeInTheDocument();
  });

  it('a_collection_without_connected_endpoints_gets_a_plain_message', async () => {
    vi.mocked(tauriApi.previewOpenApiChangeLink).mockResolvedValue(
      preview({ matched: 0, removed: 0, added: 3 }),
    );
    const user = renderForm();
    await check(user);
    expect(
      await screen.findByText(
        'This spec has no connected endpoints to compare. It will be reviewed in Spec updates.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Only \d+ of/)).not.toBeInTheDocument();
  });

  it('the_same_spec_says_so', async () => {
    vi.mocked(tauriApi.previewOpenApiChangeLink).mockResolvedValue(preview({ sameSpec: true }));
    const user = renderForm();
    await check(user);
    expect(await screen.findByText('The spec is the same.')).toBeInTheDocument();
  });

  it('the_token_field_shows_for_github_links_only', async () => {
    const user = renderForm();
    await user.type(screen.getByLabelText('New spec link'), NEW_LINK);
    expect(screen.queryByLabelText('Access token (optional)')).not.toBeInTheDocument();
    await user.clear(screen.getByLabelText('New spec link'));
    await user.type(screen.getByLabelText('New spec link'), GH_LINK);
    expect(screen.getByLabelText('Access token (optional)')).toBeInTheDocument();
  });

  it('confirm_calls_the_hook_with_the_previewed_hash', async () => {
    const onDone = vi.fn();
    const user = renderForm({ onDone });
    await check(user);
    await user.click(await screen.findByRole('button', { name: 'Change link' }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(tauriApi.confirmOpenApiChangeLink).toHaveBeenCalledWith(
      'Orders',
      NEW_LINK,
      undefined,
      'sha-1',
    );
  });

  it('a_stale_error_asks_to_check_again', async () => {
    vi.mocked(tauriApi.confirmOpenApiChangeLink).mockRejectedValue(
      'The spec changed. Run the preview again.',
    );
    const user = renderForm();
    await check(user);
    await user.click(await screen.findByRole('button', { name: 'Change link' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The spec changed. Run the preview again.',
    );
    expect(screen.queryByRole('button', { name: 'Change link' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Check link' })).toBeInTheDocument();
  });

  it('a_failed_preview_shows_only_a_fixed_sentence', async () => {
    vi.mocked(tauriApi.previewOpenApiChangeLink).mockRejectedValue(
      'cannot read /home/me/secret.yml',
    );
    const user = renderForm();
    await check(user);
    const alert = await screen.findByRole('alert');
    expect(alert).not.toHaveTextContent('secret');
    expect(alert).toHaveTextContent('Something went wrong with the OpenAPI sync. Try again.');
  });

  it('reports_busy_while_confirming', async () => {
    let finish: (v: tauriApi.OpenApiChangeLinkOutcome) => void = () => {};
    vi.mocked(tauriApi.confirmOpenApiChangeLink).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const onBusyChange = vi.fn();
    const user = renderForm({ onBusyChange });
    await check(user);
    await user.click(await screen.findByRole('button', { name: 'Change link' }));
    await waitFor(() => expect(onBusyChange).toHaveBeenLastCalledWith(true));
    finish({ pending: false, tokenKept: false, warnings: [] });
    await waitFor(() => expect(onBusyChange).toHaveBeenLastCalledWith(false));
  });

  it('a_second_click_on_change_link_sends_one_call', async () => {
    vi.mocked(tauriApi.confirmOpenApiChangeLink).mockReturnValue(new Promise(() => {}));
    const user = renderForm();
    await check(user);
    const button = await screen.findByRole('button', { name: 'Change link' });
    await user.click(button);
    await user.click(button);
    expect(tauriApi.confirmOpenApiChangeLink).toHaveBeenCalledTimes(1);
  });
});
