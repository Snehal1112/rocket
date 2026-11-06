import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ListRow } from '@/components/network/ListRow';
import { NetworkIdentity } from '@/components/network/NetworkIdentity';
import { SectionLabel } from '@/components/network/SectionLabel';

describe('ListRow', () => {
  it('is a list item with the row layout and an extra class', () => {
    render(
      <ul>
        <ListRow className='px-3'>Row</ListRow>
      </ul>,
    );
    const row = screen.getByRole('listitem');
    expect(row).toHaveClass('flex', 'items-center', 'gap-2', 'px-3');
    expect(row).toHaveTextContent('Row');
  });
});

describe('NetworkIdentity', () => {
  it('shows the name and the short id', () => {
    render(<NetworkIdentity name='Ben Ortiz' shortId='AAAA-1111' />);
    expect(screen.getByText('Ben Ortiz')).toBeInTheDocument();
    expect(screen.getByText('AAAA-1111')).toBeInTheDocument();
  });

  it('adds the extra text after the short id in one line', () => {
    render(<NetworkIdentity name='Eli Stone' shortId='ZZZZ-9999' extra='482 913' />);
    expect(screen.getByText('ZZZZ-9999 · 482 913')).toBeInTheDocument();
  });
});

describe('SectionLabel', () => {
  it('is a level 3 heading by default', () => {
    render(<SectionLabel>Paired (2)</SectionLabel>);
    expect(screen.getByRole('heading', { level: 3, name: 'Paired (2)' })).toBeInTheDocument();
  });

  it('can be a plain paragraph', () => {
    render(<SectionLabel as='p'>Team network</SectionLabel>);
    expect(screen.queryByRole('heading')).not.toBeInTheDocument();
    expect(screen.getByText('Team network')).toHaveClass('uppercase', 'tracking-wider');
  });
});
