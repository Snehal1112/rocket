import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findScriptTab } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { TabBar } from '../TabBar';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    readScriptFile: vi.fn().mockResolvedValue('x'),
    getCollection: vi.fn().mockResolvedValue({
      name: 'demo',
      settings: { headers: [], variables: [] },
      root: { uid: 'r', name: 'demo', items: [] },
    }),
  };
});

describe('TabBar new-tab menu', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('opens a blank runner tab from the new-tab context menu', () => {
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    render(<TabBar node={root} />);

    fireEvent.contextMenu(screen.getByLabelText('New request'));
    fireEvent.click(screen.getByText('Runner'));

    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    expect(updated.tabs.some((t) => t.tabType === 'runner')).toBe(true);
  });
});

describe('TabBar script tabs', () => {
  beforeEach(() => usePaneStore.getState().reset());

  async function renderWithScript() {
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    render(<TabBar node={root} />);
  }

  it('does not start an inline rename on double-click', async () => {
    await renderWithScript();
    fireEvent.doubleClick(screen.getByText('a.js'));
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/a.js')).not.toBeNull();
  });

  it('disables Rename in the context menu', async () => {
    await renderWithScript();
    fireEvent.contextMenu(screen.getByText('a.js'));
    const item = screen.getByText('Rename').closest('[role="menuitem"]');
    expect(item).toHaveAttribute('data-disabled');
  });
});
