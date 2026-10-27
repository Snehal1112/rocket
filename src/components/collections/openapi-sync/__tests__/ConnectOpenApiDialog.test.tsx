import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { createDeferred } from '@/test/deferred';
import { ConnectOpenApiDialog } from '../ConnectOpenApiDialog';
import { makeConnectPreview } from './fixtures';

vi.mock('sonner', () => ({ toast: { success: vi.fn(), info: vi.fn(), error: vi.fn() } }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, previewOpenApiConnect: vi.fn(), confirmOpenApiConnect: vi.fn() };
});

const LINK = 'https://example.com/openapi.json';
const GITHUB = 'https://github.com/acme/api/blob/main/openapi.yaml';

function renderDialog(open = true, onOpenChange = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const ui = (o: boolean) => (
    <QueryClientProvider client={client}>
      <ConnectOpenApiDialog collection='Orders' open={o} onOpenChange={onOpenChange} />
    </QueryClientProvider>
  );
  const result = render(ui(open));
  return { ...result, onOpenChange, setOpen: (o: boolean) => result.rerender(ui(o)) };
}

async function toStepTwo(user: ReturnType<typeof userEvent.setup>, link = LINK) {
  await user.type(screen.getByLabelText('Spec link'), link);
  await user.click(screen.getByRole('button', { name: 'Preview' }));
  await screen.findByText('1 linked, 1 added, 1 skipped');
}

describe('ConnectOpenApiDialog', () => {
  beforeAll(() => {
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
    HTMLElement.prototype.setPointerCapture = vi.fn();
    HTMLElement.prototype.releasePointerCapture = vi.fn();
    HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    vi.mocked(tauriApi.previewOpenApiConnect).mockResolvedValue(makeConnectPreview());
    vi.mocked(tauriApi.confirmOpenApiConnect).mockResolvedValue({
      linked: 1,
      added: 1,
      skipped: 1,
      warnings: [],
    });
  });

  it('preview_is_disabled_without_a_link', () => {
    renderDialog();
    expect(screen.getByRole('button', { name: 'Preview' })).toBeDisabled();
  });

  it('the_token_field_shows_for_github_links_only', async () => {
    const user = userEvent.setup();
    renderDialog();
    expect(screen.queryByLabelText(/Access token/)).toBeNull();
    await user.type(screen.getByLabelText('Spec link'), LINK);
    expect(screen.queryByLabelText(/Access token/)).toBeNull();
    await user.clear(screen.getByLabelText('Spec link'));
    await user.type(screen.getByLabelText('Spec link'), GITHUB);
    expect(screen.getByLabelText(/Access token/)).toBeInTheDocument();
  });

  it('preview_failure_shows_a_fixed_sentence', async () => {
    vi.mocked(tauriApi.previewOpenApiConnect).mockRejectedValue('boom at /home/me/secret.yml');
    const user = userEvent.setup();
    renderDialog();
    await user.type(screen.getByLabelText('Spec link'), LINK);
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    expect(await screen.findByText(/Something went wrong with the OpenAPI sync/)).toBeInTheDocument();
    expect(screen.queryByText(/secret\.yml/)).toBeNull();
  });

  it('continue_is_blocked_until_ambiguous_rows_are_picked', async () => {
    const base = makeConnectPreview();
    vi.mocked(tauriApi.previewOpenApiConnect).mockResolvedValue(
      makeConnectPreview({ ambiguous: [{ requests: base.yoursOnly, operations: base.inSpecOnly }] }),
    );
    const user = userEvent.setup();
    renderDialog();
    await user.type(screen.getByLabelText('Spec link'), LINK);
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    const connect = await screen.findByRole('button', { name: 'Connect' });
    expect(connect).toBeDisabled();
    await user.click(screen.getByRole('combobox', { name: 'Choice for tangled group 1' }));
    await user.click(await screen.findByRole('option', { name: 'Keep mine, skip the spec ones' }));
    expect(connect).toBeEnabled();
  });

  it('connect_sends_the_rows_digest_link_and_group_by', async () => {
    const user = userEvent.setup();
    renderDialog();
    await toStepTwo(user);
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    await waitFor(() => expect(tauriApi.confirmOpenApiConnect).toHaveBeenCalledTimes(1));
    expect(tauriApi.confirmOpenApiConnect).toHaveBeenCalledWith({
      collection: 'Orders',
      link: LINK,
      rows: [
        { kind: 'link', uid: 'r1', key: 'GET /pets' },
        { kind: 'keep', uid: 'r2' },
        { kind: 'skip', key: 'GET /pets/all' },
        { kind: 'add', key: 'POST /pets' },
        { kind: 'keep', uid: 'r3' },
      ],
      groupBy: 'tags',
      interval: 'every_5_minutes',
      digest: 'digest-1',
      specSha256: 'sha-1',
    });
  });

  it('success_opens_the_sync_tab_and_closes', async () => {
    const open = vi.spyOn(usePaneStore.getState(), 'openOpenApiSyncTab');
    const user = userEvent.setup();
    const { onOpenChange } = renderDialog();
    await toStepTwo(user);
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    await waitFor(() => expect(open).toHaveBeenCalledWith('Orders'));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('a_stale_digest_returns_to_step_one_with_the_sentence', async () => {
    vi.mocked(tauriApi.confirmOpenApiConnect).mockRejectedValue(
      'The collection changed. Run the preview again.',
    );
    const user = userEvent.setup();
    renderDialog();
    await toStepTwo(user);
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    expect(
      await screen.findByText('The collection changed. Run the preview again.'),
    ).toBeInTheDocument();
    expect(screen.getByLabelText('Spec link')).toHaveValue(LINK);
  });

  it('the_token_is_not_kept_after_close', async () => {
    const user = userEvent.setup();
    const { setOpen } = renderDialog();
    await user.type(screen.getByLabelText('Spec link'), GITHUB);
    await user.type(screen.getByLabelText(/Access token/), 'ghp_secret');
    setOpen(false);
    setOpen(true);
    expect(screen.getByLabelText('Spec link')).toHaveValue('');
    await userEvent.setup().type(screen.getByLabelText('Spec link'), GITHUB);
    expect(screen.getByLabelText(/Access token/)).toHaveValue('');
  });

  it('back_keeps_the_link', async () => {
    const user = userEvent.setup();
    renderDialog();
    await toStepTwo(user);
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByLabelText('Spec link')).toHaveValue(LINK);
  });

  it('warnings_are_toasted_as_fixed_sentences', async () => {
    vi.mocked(tauriApi.confirmOpenApiConnect).mockResolvedValue({
      linked: 1,
      added: 1,
      skipped: 1,
      warnings: ['Checks are not enabled yet.', 'keychain failed at /home/me/.x'],
    });
    const user = userEvent.setup();
    renderDialog();
    await toStepTwo(user);
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    await waitFor(() => expect(toast.info).toHaveBeenCalledWith('Checks are not enabled yet.'));
    const all = JSON.stringify(vi.mocked(toast.info).mock.calls);
    expect(all).not.toContain('/home/me');
  });

  it('connect_ignores_a_second_click_while_running', async () => {
    const pending = createDeferred<tauriApi.ConnectOutcome>();
    vi.mocked(tauriApi.confirmOpenApiConnect).mockReturnValue(pending.promise);
    const user = userEvent.setup();
    renderDialog();
    await toStepTwo(user);
    const connect = screen.getByRole('button', { name: 'Connect' });
    await user.click(connect);
    await user.click(connect);
    expect(tauriApi.confirmOpenApiConnect).toHaveBeenCalledTimes(1);
    pending.resolve({ linked: 0, added: 0, skipped: 0, warnings: [] });
  });
});
