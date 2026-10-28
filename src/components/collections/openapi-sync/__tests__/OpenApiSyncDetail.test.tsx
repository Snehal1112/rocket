import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncDetail } from '../OpenApiSyncDetail';
import { makePlan, newItem, removedItem } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, deleteOpenApiRequests: vi.fn() };
});

function props(overrides: Partial<React.ComponentProps<typeof OpenApiSyncDetail>> = {}) {
  return {
    collection: 'c',
    plan: makePlan(),
    selectedId: null,
    choices: new Map<string, Record<string, 'spec' | 'yours'>>(),
    links: [] as Array<[string, string]>,
    onChoose: vi.fn(),
    onLink: vi.fn(),
    onUnlink: vi.fn(),
    linksLocked: false,
    ...overrides,
  };
}

describe('OpenApiSyncDetail', () => {
  beforeEach(() => vi.clearAllMocks());

  describe('delete', () => {
    const planWith = (edited: boolean | null) =>
      makePlan({ items: [{ ...removedItem, edited } as never] });

    it('a_removed_edited_request_offers_delete_in_its_detail', async () => {
      vi.mocked(tauriApi.deleteOpenApiRequests).mockResolvedValue(1);
      renderWithQuery(
        <OpenApiSyncDetail {...props({ plan: planWith(true), selectedId: 'removed:DELETE /pets' })} />,
      );
      const user = userEvent.setup();
      await user.click(screen.getByRole('button', { name: 'Delete this request' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
      );
      await waitFor(() =>
        expect(tauriApi.deleteOpenApiRequests).toHaveBeenCalledWith('c', [
          { uid: 'u-removed', requestPath: 'pets/delete-pet.yml' },
        ]),
      );
    });

    it('keeps_the_confirm_dialog_open_and_busy_until_the_delete_settles', async () => {
      let finish: (n: number) => void = () => {};
      vi.mocked(tauriApi.deleteOpenApiRequests).mockReturnValue(
        new Promise<number>((resolve) => {
          finish = resolve;
        }),
      );
      renderWithQuery(
        <OpenApiSyncDetail {...props({ plan: planWith(true), selectedId: 'removed:DELETE /pets' })} />,
      );
      const user = userEvent.setup();
      await user.click(screen.getByRole('button', { name: 'Delete this request' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
      );
      const busy = await within(screen.getByRole('alertdialog')).findByRole('button', {
        name: 'Deleting...',
      });
      expect(busy).toBeDisabled();
      expect(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Cancel' })).toBeDisabled();
      finish(1);
      await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull());
    });

    it('an_unedited_removed_request_offers_no_delete', () => {
      renderWithQuery(
        <OpenApiSyncDetail {...props({ plan: planWith(false), selectedId: 'removed:DELETE /pets' })} />,
      );
      expect(screen.queryByRole('button', { name: 'Delete this request' })).toBeNull();
    });

    it('a_removed_request_of_unknown_edit_state_offers_no_delete', () => {
      renderWithQuery(
        <OpenApiSyncDetail {...props({ plan: planWith(null), selectedId: 'removed:DELETE /pets' })} />,
      );
      expect(screen.queryByRole('button', { name: 'Delete this request' })).toBeNull();
    });
  });

  it('shows each changed field with baseline, yours, spec and the outcome', () => {
    renderWithQuery(<OpenApiSyncDetail {...props({ selectedId: 'changed:POST /pets' })} />);
    expect(screen.getByText('pets/add-pet.yml')).toBeInTheDocument();
    expect(screen.getByText('Header Accept')).toBeInTheDocument();
    expect(screen.getByText('Body $.a.b')).toBeInTheDocument();
    expect(screen.getByText('Spec wins')).toBeInTheDocument();
    expect(screen.getByText('Yours kept')).toBeInTheDocument();
    expect(screen.getByText('Conflict')).toBeInTheDocument();
    expect(screen.getAllByText('Baseline')).toHaveLength(3);
    expect(screen.getByText('a/c')).toBeInTheDocument();
    expect(screen.getByText('u2')).toBeInTheDocument();
  });

  it('shows the request summary and folder of a new item', () => {
    renderWithQuery(<OpenApiSyncDetail {...props({ selectedId: 'new:GET /pets' })} />);
    expect(screen.getByText('List pets')).toBeInTheDocument();
    expect(screen.getByText('Folder: pets / read')).toBeInTheDocument();
    expect(screen.getByText('Possibly renamed from GET /animals')).toBeInTheDocument();
  });

  it('says a removed item of unknown edit state is kept', () => {
    renderWithQuery(<OpenApiSyncDetail {...props({ selectedId: 'removed:DELETE /pets' })} />);
    expect(screen.getByText('Treated as edited, will be kept.')).toBeInTheDocument();
  });

  it('says an unedited removed request will be deleted when it can be rewritten', () => {
    const plan = makePlan({
      items: [{ ...makePlan().items[0], edited: false } as never],
    });
    renderWithQuery(<OpenApiSyncDetail {...props({ plan, selectedId: 'removed:DELETE /pets' })} />);
    expect(
      screen.getByText('You did not edit it, so it will be deleted if Rocket can rewrite it.'),
    ).toBeInTheDocument();
  });

  it('shows the cautious warning', () => {
    renderWithQuery(
      <OpenApiSyncDetail
        {...props({ plan: makePlan({ cautious: true }), selectedId: 'new:GET /pets' })}
      />,
    );
    expect(screen.getByRole('alert')).toHaveTextContent(/plays safe/);
  });

  it('renders only the selected item', () => {
    renderWithQuery(<OpenApiSyncDetail {...props({ selectedId: 'new:GET /pets' })} />);
    expect(screen.queryByText('Add pet')).toBeNull();
    expect(screen.queryByText('Delete pet')).toBeNull();
  });

  it('defaults each field choice from its outcome, conflicts to yours', () => {
    renderWithQuery(<OpenApiSyncDetail {...props({ selectedId: 'changed:POST /pets' })} />);
    const accept = screen.getByRole('radiogroup', { name: 'Header Accept' });
    expect(within(accept).getByRole('radio', { name: 'Take spec' })).toBeChecked();
    const body = screen.getByRole('radiogroup', { name: 'Body $.a.b' });
    expect(within(body).getByRole('radio', { name: 'Keep yours' })).toBeChecked();
    const url = screen.getByRole('radiogroup', { name: 'URL' });
    expect(within(url).getByRole('radio', { name: 'Keep yours' })).toBeChecked();
  });

  it('reports a field choice and shows an explicit choice', async () => {
    const p = props({
      selectedId: 'changed:POST /pets',
      choices: new Map([['POST /pets', { url: 'spec' as const }]]),
    });
    renderWithQuery(<OpenApiSyncDetail {...p} />);
    const url = screen.getByRole('radiogroup', { name: 'URL' });
    expect(within(url).getByRole('radio', { name: 'Take spec' })).toBeChecked();
    await userEvent.setup().click(within(url).getByRole('radio', { name: 'Keep yours' }));
    expect(p.onChoose).toHaveBeenCalledWith('POST /pets', 'url', 'yours');
  });

  it('links a rename from the new side and from the removed side', async () => {
    const p = props({ selectedId: 'new:GET /pets' });
    const { rerender } = renderWithQuery(<OpenApiSyncDetail {...p} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Link as rename' }));
    expect(p.onLink).toHaveBeenCalledWith('GET /animals', 'GET /pets');
    rerender(<OpenApiSyncDetail {...p} selectedId='removed:DELETE /pets' />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Link as rename' }));
    expect(p.onLink).toHaveBeenCalledWith('DELETE /pets', 'DELETE /animals');
  });

  it('offers no link button without a rename hint', () => {
    const plan = makePlan({
      items: [{ ...makePlan().items[2], possiblyRenamedFrom: undefined } as never],
    });
    renderWithQuery(<OpenApiSyncDetail {...props({ plan, selectedId: 'new:GET /pets' })} />);
    expect(screen.queryByRole('button', { name: 'Link as rename' })).toBeNull();
  });

  it('disables Link and Unlink while locked', () => {
    const { rerender } = renderWithQuery(
      <OpenApiSyncDetail {...props({ selectedId: 'new:GET /pets', linksLocked: true })} />,
    );
    expect(screen.getByRole('button', { name: 'Link as rename' })).toBeDisabled();
    rerender(
      <OpenApiSyncDetail
        {...props({
          selectedId: 'changed:POST /pets',
          links: [['GET /old', 'POST /pets']],
          linksLocked: true,
        })}
      />,
    );
    expect(screen.getByRole('button', { name: 'Unlink rename' })).toBeDisabled();
  });

  it('offers to unlink a changed item that came from a link', async () => {
    const p = props({
      selectedId: 'changed:POST /pets',
      links: [['GET /old', 'POST /pets']],
    });
    renderWithQuery(<OpenApiSyncDetail {...p} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Unlink rename' }));
    expect(p.onUnlink).toHaveBeenCalledWith('GET /old');
  });

  it('explains that an edited variable value is kept', () => {
    const plan = makePlan({
      items: [],
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'v', spec: 'https://x' }],
    });
    renderWithQuery(<OpenApiSyncDetail {...props({ plan, selectedId: 'variable:baseUrl:baseUrl' })} />);
    expect(
      screen.getByText(/If you changed this value yourself, Rocket keeps yours.*no trusted baseline/),
    ).toBeInTheDocument();
  });
  it('a_new_item_shows_its_preview', () => {
    const item = {
      ...newItem,
      preview: {
        method: 'POST',
        url: '{{baseUrl}}/pets',
        headers: ['Accept', 'X-Trace'],
        query: ['limit'],
        hasBody: true,
        authKind: 'bearer',
      },
    };
    const plan = makePlan({ items: [item] });
    renderWithQuery(<OpenApiSyncDetail {...props({ plan, selectedId: 'new:GET /pets' })} />);
    expect(screen.getByText('POST')).toBeInTheDocument();
    expect(screen.getByText('{{baseUrl}}/pets')).toBeInTheDocument();
    expect(screen.getByText('Headers: Accept, X-Trace')).toBeInTheDocument();
    expect(screen.getByText('Query: limit')).toBeInTheDocument();
    expect(screen.getByText('Has a body')).toBeInTheDocument();
    expect(screen.getByText('Auth: bearer')).toBeInTheDocument();
  });

  it('a_new_item_without_extras_says_so', () => {
    const item = {
      ...newItem,
      preview: {
        method: 'GET',
        url: '/ping',
        headers: [],
        query: [],
        hasBody: false,
        authKind: null,
      },
    };
    const plan = makePlan({ items: [item] });
    renderWithQuery(<OpenApiSyncDetail {...props({ plan, selectedId: 'new:GET /pets' })} />);
    expect(screen.getByText('No headers')).toBeInTheDocument();
    expect(screen.getByText('No query parameters')).toBeInTheDocument();
    expect(screen.getByText('No body')).toBeInTheDocument();
    expect(screen.getByText('No auth of its own')).toBeInTheDocument();
  });
});
