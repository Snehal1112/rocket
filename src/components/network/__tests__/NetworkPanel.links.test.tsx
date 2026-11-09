import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkPanel } from '@/components/network/NetworkPanel';
import { useNetworkStore } from '@/stores/network-store';
import { makeLink, makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

describe('NetworkPanel links', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('shows the linked collections and passes Unlink to the host', async () => {
    useNetworkStore.setState({
      status: makeStatus(),
      nodes: [makeNode({ group: 'paired', status: 'online' })],
      links: [makeLink()],
    });
    const onUnlink = vi.fn();
    render(<NetworkPanel onAddAddress={vi.fn()} onRemove={vi.fn()} onUnlink={onUnlink} />);
    expect(screen.getByText('Linked collections (1)')).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Unlink Orders from Ben Ortiz (AAAA-1111)' }),
    );
    expect(onUnlink).toHaveBeenCalledWith(makeLink());
  });

  it('shows no linked section without links', () => {
    useNetworkStore.setState({ status: makeStatus(), nodes: [makeNode()], links: [] });
    render(<NetworkPanel onAddAddress={vi.fn()} onRemove={vi.fn()} onUnlink={vi.fn()} />);
    expect(screen.queryByText(/Linked collections/)).toBeNull();
  });
});
