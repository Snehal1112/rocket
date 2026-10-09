import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ComposerChip } from '../chips';
import { ComposerChips } from '../ComposerChips';

const CHIPS: ComposerChip[] = [
  {
    key: 'request:shop:list.yml',
    item: { kind: 'request', collection: 'shop', path: 'list.yml', label: 'List orders' },
    focus: true,
  },
  {
    key: 'environment:shop:dev',
    item: { kind: 'environment', collection: 'shop', path: 'dev', label: 'env: dev' },
    focus: false,
  },
];

describe('ComposerChips', () => {
  it('renders nothing without chips', () => {
    const { container } = render(<ComposerChips chips={[]} onRemove={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows each chip and removes one by key', async () => {
    const onRemove = vi.fn();
    render(<ComposerChips chips={CHIPS} onRemove={onRemove} />);
    expect(screen.getByText('List orders')).toBeInTheDocument();
    expect(screen.getByText('env: dev')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Remove env: dev' }));
    expect(onRemove).toHaveBeenCalledWith('environment:shop:dev');
  });
});
