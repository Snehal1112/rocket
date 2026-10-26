import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SpecSyncDetail } from '../SpecSyncDetail';
import { makePlan } from './fixtures';

function props(overrides: Partial<React.ComponentProps<typeof SpecSyncDetail>> = {}) {
  return {
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

describe('SpecSyncDetail', () => {
  it('shows each changed field with baseline, yours, spec and the outcome', () => {
    render(<SpecSyncDetail {...props({ selectedId: 'changed:POST /pets' })} />);
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
    render(<SpecSyncDetail {...props({ selectedId: 'new:GET /pets' })} />);
    expect(screen.getByText('List pets')).toBeInTheDocument();
    expect(screen.getByText('Folder: pets / read')).toBeInTheDocument();
    expect(screen.getByText('Possibly renamed from GET /animals')).toBeInTheDocument();
  });

  it('says a removed item of unknown edit state is kept', () => {
    render(<SpecSyncDetail {...props({ selectedId: 'removed:DELETE /pets' })} />);
    expect(screen.getByText('Treated as edited, will be kept.')).toBeInTheDocument();
  });

  it('says an unedited removed request will be deleted when it can be rewritten', () => {
    const plan = makePlan({
      items: [{ ...makePlan().items[0], edited: false } as never],
    });
    render(<SpecSyncDetail {...props({ plan, selectedId: 'removed:DELETE /pets' })} />);
    expect(
      screen.getByText('You did not edit it, so it will be deleted if Rocket can rewrite it.'),
    ).toBeInTheDocument();
  });

  it('shows the cautious warning', () => {
    render(
      <SpecSyncDetail
        {...props({ plan: makePlan({ cautious: true }), selectedId: 'new:GET /pets' })}
      />,
    );
    expect(screen.getByRole('alert')).toHaveTextContent(/plays safe/);
  });

  it('renders only the selected item', () => {
    render(<SpecSyncDetail {...props({ selectedId: 'new:GET /pets' })} />);
    expect(screen.queryByText('Add pet')).toBeNull();
    expect(screen.queryByText('Delete pet')).toBeNull();
  });

  it('defaults each field choice from its outcome, conflicts to yours', () => {
    render(<SpecSyncDetail {...props({ selectedId: 'changed:POST /pets' })} />);
    const accept = screen.getByRole('radiogroup', { name: 'Header Accept' });
    expect(within(accept).getByRole('radio', { name: 'Take spec' })).toBeChecked();
    const body = screen.getByRole('radiogroup', { name: 'Body $.a.b' });
    expect(within(body).getByRole('radio', { name: 'Keep yours' })).toBeChecked();
    const url = screen.getByRole('radiogroup', { name: 'Url' });
    expect(within(url).getByRole('radio', { name: 'Keep yours' })).toBeChecked();
  });

  it('reports a field choice and shows an explicit choice', async () => {
    const p = props({
      selectedId: 'changed:POST /pets',
      choices: new Map([['POST /pets', { url: 'spec' as const }]]),
    });
    render(<SpecSyncDetail {...p} />);
    const url = screen.getByRole('radiogroup', { name: 'Url' });
    expect(within(url).getByRole('radio', { name: 'Take spec' })).toBeChecked();
    await userEvent.setup().click(within(url).getByRole('radio', { name: 'Keep yours' }));
    expect(p.onChoose).toHaveBeenCalledWith('POST /pets', 'url', 'yours');
  });

  it('links a rename from the new side and from the removed side', async () => {
    const p = props({ selectedId: 'new:GET /pets' });
    const { rerender } = render(<SpecSyncDetail {...p} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Link as rename' }));
    expect(p.onLink).toHaveBeenCalledWith('GET /animals', 'GET /pets');
    rerender(<SpecSyncDetail {...p} selectedId='removed:DELETE /pets' />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Link as rename' }));
    expect(p.onLink).toHaveBeenCalledWith('DELETE /pets', 'DELETE /animals');
  });

  it('offers no link button without a rename hint', () => {
    const plan = makePlan({
      items: [{ ...makePlan().items[2], possiblyRenamedFrom: undefined } as never],
    });
    render(<SpecSyncDetail {...props({ plan, selectedId: 'new:GET /pets' })} />);
    expect(screen.queryByRole('button', { name: 'Link as rename' })).toBeNull();
  });

  it('disables Link and Unlink while locked', () => {
    const { rerender } = render(
      <SpecSyncDetail {...props({ selectedId: 'new:GET /pets', linksLocked: true })} />,
    );
    expect(screen.getByRole('button', { name: 'Link as rename' })).toBeDisabled();
    rerender(
      <SpecSyncDetail
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
    render(<SpecSyncDetail {...p} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Unlink rename' }));
    expect(p.onUnlink).toHaveBeenCalledWith('GET /old');
  });

  it('explains that an edited variable value is kept', () => {
    const plan = makePlan({
      items: [],
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'v', spec: 'https://x' }],
    });
    render(<SpecSyncDetail {...props({ plan, selectedId: 'variable:baseUrl:baseUrl' })} />);
    expect(
      screen.getByText(/If you changed this value yourself, Rocket keeps yours/),
    ).toBeInTheDocument();
  });
});
