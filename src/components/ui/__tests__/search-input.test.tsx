import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SearchInput } from '@/components/ui/search-input';

describe('SearchInput', () => {
  it('is a search box with the label and the placeholder', () => {
    render(<SearchInput value='' onChange={vi.fn()} label='Search nodes' placeholder='Find' />);
    const box = screen.getByRole('searchbox', { name: 'Search nodes' });
    expect(box).toHaveAttribute('placeholder', 'Find');
  });

  it('shows the value it is given', () => {
    render(<SearchInput value='ana' onChange={vi.fn()} label='Search nodes' />);
    expect(screen.getByRole('searchbox')).toHaveValue('ana');
  });

  it('hands the typed text, not the event, to onChange', async () => {
    const onChange = vi.fn();
    render(<SearchInput value='' onChange={onChange} label='Search nodes' />);
    await userEvent.type(screen.getByRole('searchbox'), 'ab');
    expect(onChange).toHaveBeenNthCalledWith(1, 'a');
    expect(onChange).toHaveBeenNthCalledWith(2, 'b');
  });

  it('puts the class name on the wrapper', () => {
    const { container } = render(
      <SearchInput value='' onChange={vi.fn()} label='Search nodes' className='w-64' />,
    );
    expect(container.firstElementChild).toHaveClass('relative', 'w-64');
  });
});
