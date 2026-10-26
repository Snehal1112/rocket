import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { OpenApiSyncItem } from '@/lib/tauri-api';
import { ApplyConfirmDialog } from '../ApplyConfirmDialog';
import { changedItem, newItem, removedItem } from './fixtures';

const removed = (key: string, edited: boolean | null): OpenApiSyncItem =>
  ({ ...removedItem, key, edited }) as OpenApiSyncItem;

function renderDialog(items: OpenApiSyncItem[], variableCount = 0, onConfirm = vi.fn()) {
  render(
    <ApplyConfirmDialog
      open
      onOpenChange={vi.fn()}
      items={items}
      variableCount={variableCount}
      onConfirm={onConfirm}
    />,
  );
  return onConfirm;
}

describe('ApplyConfirmDialog', () => {
  it('lists the counts of new, changed and removed requests', () => {
    renderDialog([newItem, changedItem, removed('a', false)], 2);
    expect(screen.getByText('1 new request will be added.')).toBeInTheDocument();
    expect(screen.getByText('1 changed request will be updated.')).toBeInTheDocument();
    expect(screen.getByText('2 variable changes will be applied. A value you changed yourself is kept.')).toBeInTheDocument();
  });

  it('says which removed requests are deleted and which are kept', () => {
    renderDialog([removed('a', false), removed('b', false), removed('c', true), removed('d', null)]);
    expect(
      screen.getByText(
        '2 removed requests will be deleted if Rocket can rewrite them, because you did not edit them.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        '2 removed requests stay in your collection but are no longer tracked, because you edited them or Rocket cannot tell.',
      ),
    ).toBeInTheDocument();
  });

  it('uses the singular for one variable change', () => {
    renderDialog([newItem], 1);
    expect(
      screen.getByText('1 variable change will be applied. A value you changed yourself is kept.'),
    ).toBeInTheDocument();
  });

  it('leaves out lines with a zero count', () => {
    renderDialog([newItem]);
    expect(screen.queryByText(/changed request/)).toBeNull();
    expect(screen.queryByText(/removed request/)).toBeNull();
    expect(screen.queryByText(/variable/)).toBeNull();
  });

  it('disables Apply now when nothing is selected', () => {
    renderDialog([], 0);
    expect(screen.getByRole('button', { name: 'Apply now' })).toBeDisabled();
  });

  it('confirms through the Apply now button', async () => {
    const onConfirm = renderDialog([newItem]);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Apply now' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
