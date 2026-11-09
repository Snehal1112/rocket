import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingSyncToast, incomingSyncToastId } from '@/components/network/IncomingSyncToast';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({ toast: { dismiss: vi.fn() } }));
vi.mock('sonner', () => ({ toast: mocks.toast }));
vi.mock('@/lib/tauri-api', () => ({}));

const props = {
  syncId: 's1',
  kind: 'review' as const,
  displayName: 'Ben Ortiz',
  shortId: 'AAAA-1111',
  collectionName: 'Orders "v2"',
};

describe('IncomingSyncToast', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('names the sender, the collection and the short id', () => {
    render(<IncomingSyncToast {...props} />);
    expect(screen.getByText('Ben Ortiz wants to sync a collection')).toBeInTheDocument();
    expect(screen.getByText('Orders "v2"')).toBeInTheDocument();
    expect(screen.getByText('AAAA-1111')).toBeInTheDocument();
  });

  it('falls back when the display name is empty', () => {
    render(<IncomingSyncToast {...props} displayName='' />);
    expect(screen.getByText('Unnamed node wants to sync a collection')).toBeInTheDocument();
  });

  it('words a confirm differently', () => {
    render(<IncomingSyncToast {...props} kind='confirm' />);
    expect(screen.getByText('Ben Ortiz has changes ready to apply')).toBeInTheDocument();
  });

  it('has only a Review action that opens the dialog and dismisses the toast', async () => {
    render(<IncomingSyncToast {...props} />);
    expect(screen.getAllByRole('button')).toHaveLength(1);
    await userEvent.click(screen.getByRole('button', { name: 'Review' }));
    expect(useNetworkStore.getState().dialogSyncId).toBe('s1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith(incomingSyncToastId('s1'));
  });
});
