import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InitialsAvatar } from '@/components/ui/initials-avatar';
import { avatarColorForName } from '@/lib/contracts/avatarColor';

describe('InitialsAvatar', () => {
  it('shows the initials it is given', () => {
    render(<InitialsAvatar name='Ana Lima' initials='AL' />);
    expect(screen.getByText('AL')).toBeInTheDocument();
  });

  it('works out the initials from the name when none are given', () => {
    render(<InitialsAvatar name='Dana Roy' />);
    expect(screen.getByText('DR')).toBeInTheDocument();
  });

  it('uses the same colour for the same name', () => {
    render(<InitialsAvatar name='Ana Lima' initials='AL' />);
    expect(screen.getByText('AL')).toHaveStyle({
      backgroundColor: avatarColorForName('Ana Lima'),
    });
  });

  it('defaults to 28 pixels and takes another size', () => {
    const { rerender } = render(<InitialsAvatar name='Ana Lima' initials='AL' />);
    expect(screen.getByText('AL')).toHaveStyle({ width: '28px', height: '28px' });
    rerender(<InitialsAvatar name='Ana Lima' initials='AL' size={20} />);
    expect(screen.getByText('AL')).toHaveStyle({ width: '20px', height: '20px' });
  });

  it('is hidden from assistive technology', () => {
    render(<InitialsAvatar name='Ana Lima' initials='AL' />);
    expect(screen.getByText('AL')).toHaveAttribute('aria-hidden', 'true');
  });
});
