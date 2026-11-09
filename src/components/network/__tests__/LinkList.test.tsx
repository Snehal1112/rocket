import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { LinkList } from '@/components/network/LinkList';
import { makeLink } from '@/test/network-fixtures';

describe('LinkList', () => {
  it('shows each link with its collection, node, short id and state', () => {
    render(
      <LinkList
        links={[
          makeLink({ collectionName: 'Orders "v2"', state: 'changes_waiting' }),
          makeLink({
            linkId: 'L2',
            collectionName: 'Billing',
            displayName: 'Cara Lund',
            shortId: 'BBBB-2222',
            state: 'broken',
          }),
        ]}
        onUnlink={vi.fn()}
      />,
    );
    const items = screen.getAllByRole('listitem');
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent('Orders "v2"');
    expect(items[0]).toHaveTextContent('Ben Ortiz');
    expect(items[0]).toHaveTextContent('AAAA-1111');
    expect(items[0]).toHaveTextContent('Changes waiting');
    expect(items[1]).toHaveTextContent('Broken');
  });

  it('never shows a full device id', () => {
    render(
      <LinkList links={[makeLink({ deviceId: 'K7QF2M9XPYE7HWE7ABCDEFGHIJ' })]} onUnlink={vi.fn()} />,
    );
    expect(screen.queryByText(/K7QF2M9XPYE7HWE7ABCDEFGHIJ/)).toBeNull();
  });

  it('names a node without a display name', () => {
    render(<LinkList links={[makeLink({ displayName: '' })]} onUnlink={vi.fn()} />);
    expect(screen.getByRole('listitem')).toHaveTextContent('Unnamed node');
  });

  it('calls onUnlink with the link', async () => {
    const onUnlink = vi.fn();
    const link = makeLink();
    render(<LinkList links={[link]} onUnlink={onUnlink} />);
    await userEvent.click(
      screen.getByRole('button', { name: 'Unlink Orders from Ben Ortiz (AAAA-1111)' }),
    );
    expect(onUnlink).toHaveBeenCalledWith(link);
  });

  it('renders nothing for an empty list', () => {
    const { container } = render(<LinkList links={[]} onUnlink={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });
});
