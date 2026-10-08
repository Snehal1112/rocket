import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowSearchBar } from '../FlowSearchBar';

const out = (id: string, label: string): FlowNode => ({
  id,
  kind: { kind: 'Output', label },
  position: { x: 0, y: 0 },
});

const nodes = [out('a', 'Alpha'), out('b', 'Beta'), out('c', 'Gamma')];

function setup(extra: Partial<React.ComponentProps<typeof FlowSearchBar>> = {}) {
  const onShowMatch = vi.fn();
  const onClose = vi.fn();
  const user = userEvent.setup();
  const view = render(
    <FlowSearchBar
      nodes={nodes}
      focusToken={0}
      onShowMatch={onShowMatch}
      onClose={onClose}
      {...extra}
    />,
  );
  const input = () => screen.getByRole('textbox', { name: 'Search nodes' });
  return { user, onShowMatch, onClose, input, view };
}

describe('FlowSearchBar', () => {
  it('takes focus when it opens', () => {
    const { input } = setup();
    expect(input()).toHaveFocus();
  });

  it('shows the first match as you type and counts the matches', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    expect(onShowMatch).toHaveBeenLastCalledWith('a');
    expect(screen.getByRole('status')).toHaveTextContent('1 of 3');
  });

  it('moves to the next match on Enter and wraps around', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    await user.keyboard('{Enter}');
    expect(onShowMatch).toHaveBeenLastCalledWith('b');
    expect(screen.getByRole('status')).toHaveTextContent('2 of 3');
    await user.keyboard('{Enter}{Enter}');
    expect(onShowMatch).toHaveBeenLastCalledWith('a');
    expect(screen.getByRole('status')).toHaveTextContent('1 of 3');
  });

  it('moves to the previous match on Shift+Enter and wraps to the last', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    await user.keyboard('{Shift>}{Enter}{/Shift}');
    expect(onShowMatch).toHaveBeenLastCalledWith('c');
    expect(screen.getByRole('status')).toHaveTextContent('3 of 3');
  });

  it('shows 0 of 0 and selects nothing when no node matches', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'zzz');
    expect(screen.getByRole('status')).toHaveTextContent('0 of 0');
    await user.keyboard('{Enter}');
    expect(onShowMatch).not.toHaveBeenCalled();
  });

  it('shows no counter for an empty query', () => {
    setup();
    expect(screen.getByRole('status')).toBeEmptyDOMElement();
  });

  it('closes on Escape and with the close button', async () => {
    const { user, input, onClose } = setup();
    await user.type(input(), 'a');
    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: 'Close search' }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('steps with the previous and next buttons', async () => {
    const { user, input, onShowMatch } = setup();
    await user.type(input(), 'a');
    await user.click(screen.getByRole('button', { name: 'Next match' }));
    expect(onShowMatch).toHaveBeenLastCalledWith('b');
    await user.click(screen.getByRole('button', { name: 'Previous match' }));
    expect(onShowMatch).toHaveBeenLastCalledWith('a');
  });

  it('takes focus again when the focus token changes', async () => {
    const { user, input, view, onShowMatch, onClose } = setup();
    await user.click(screen.getByRole('button', { name: 'Close search' }));
    expect(input()).not.toHaveFocus();
    view.rerender(
      <FlowSearchBar nodes={nodes} focusToken={1} onShowMatch={onShowMatch} onClose={onClose} />,
    );
    expect(input()).toHaveFocus();
  });
});
