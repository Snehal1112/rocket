import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { type FlowNodeActions, FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { NodeMenuButton } from '../NodeMenuButton';

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

function renderButton(actions: Partial<FlowNodeActions> = {}, isWait = false) {
  const value: FlowNodeActions = {
    updateNodeKind: vi.fn(),
    removeSwitchCase: vi.fn(),
    openProperties: vi.fn(),
    ...actions,
  };
  render(
    <FlowNodeActionsContext.Provider value={value}>
      <NodeMenuButton nodeId='n1' label='Login' isWait={isWait} />
    </FlowNodeActionsContext.Provider>,
  );
  return value;
}

describe('NodeMenuButton', () => {
  it('opens properties directly when there is no run to build on', async () => {
    const actions = renderButton();
    await userEvent.click(screen.getByLabelText('Edit Login'));
    expect(actions.openProperties).toHaveBeenCalledWith('n1');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('offers Run this node and Run from here once the tab has a run', async () => {
    const runNode = vi.fn();
    renderButton({ runNode });
    await userEvent.click(screen.getByLabelText('Edit Login'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Run this node' }));
    expect(runNode).toHaveBeenCalledWith('n1', 'node');
    await userEvent.click(screen.getByLabelText('Edit Login'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Run from here' }));
    expect(runNode).toHaveBeenLastCalledWith('n1', 'fromHere');
  });

  it('still opens properties from the menu', async () => {
    const actions = renderButton({ runNode: vi.fn() });
    await userEvent.click(screen.getByLabelText('Edit Login'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Edit properties' }));
    expect(actions.openProperties).toHaveBeenCalledWith('n1');
  });

  it('disables both run items while a run is in progress', async () => {
    renderButton({ runNode: vi.fn(), runBusy: true });
    await userEvent.click(screen.getByLabelText('Edit Login'));
    expect(screen.getByRole('menuitem', { name: 'Run this node' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByRole('menuitem', { name: 'Run from here' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
  });

  it('disables Run this node for a Wait node but keeps Run from here', async () => {
    renderButton({ runNode: vi.fn() }, true);
    await userEvent.click(screen.getByLabelText('Edit Login'));
    expect(screen.getByRole('menuitem', { name: 'Run this node' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByRole('menuitem', { name: 'Run from here' })).not.toHaveAttribute(
      'aria-disabled',
    );
  });
});
