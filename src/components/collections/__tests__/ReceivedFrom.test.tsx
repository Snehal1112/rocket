import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { ReceivedFromLine, ReceivedFromName } from '@/components/collections/ReceivedFrom';
import type { ReceivedCollection } from '@/lib/tauri-api';

const origin: ReceivedCollection = {
  collectionUid: 'u1',
  senderName: 'Ana Lima',
  senderShortId: '50L2-DPML',
  receivedAt: '2026-11-11T09:00:00Z',
};

describe('ReceivedFromName', () => {
  it('shows the plain name when the collection was not received', () => {
    render(<ReceivedFromName name='Orders' />);
    expect(screen.getByText('Orders')).toBeInTheDocument();
  });

  it('shows the sender, short id and date on hover', async () => {
    render(<ReceivedFromName name='Orders (from Ana Lima)' origin={origin} />);
    await userEvent.hover(screen.getByText('Orders (from Ana Lima)'));
    const tip = await screen.findByRole('tooltip');
    expect(tip).toHaveTextContent('Received from Ana Lima');
    expect(tip).toHaveTextContent('50L2-DPML');
  });
});

describe('ReceivedFromLine', () => {
  it('shows who sent the collection, when, and the short id', () => {
    render(<ReceivedFromLine origin={origin} />);
    const line = screen.getByText(/Received from Ana Lima on/);
    expect(line).toBeInTheDocument();
    expect(screen.getByText('50L2-DPML')).toBeInTheDocument();
  });
});
