import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createRef } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { StatusBarCount } from '@/components/status-bar/StatusBarCount';
import { StatusBarItem } from '@/components/status-bar/StatusBarItem';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

describe('StatusBarItem', () => {
  it('frames a button with its title and label', () => {
    render(
      <StatusBarItem title='Open it' aria-label='Item label'>
        Body
      </StatusBarItem>,
    );
    const button = screen.getByRole('button', { name: 'Item label' });
    expect(button).toHaveAttribute('title', 'Open it');
    expect(button).toHaveTextContent('Body');
    expect(button.parentElement).toHaveClass(
      'flex',
      'h-5',
      'items-center',
      'border-x',
      'border-statusbar-border',
      'px-1',
    );
  });

  it('forwards the click', async () => {
    const onClick = vi.fn();
    render(<StatusBarItem aria-label='Item label' onClick={onClick} />);
    await userEvent.click(screen.getByRole('button', { name: 'Item label' }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('forwards a ref to the button', () => {
    const ref = createRef<HTMLButtonElement>();
    render(<StatusBarItem ref={ref} aria-label='Item label' />);
    expect(ref.current).toBe(screen.getByRole('button', { name: 'Item label' }));
  });

  it('adds a class name to the button', () => {
    render(<StatusBarItem aria-label='Item label' className='opacity-50' />);
    expect(screen.getByRole('button', { name: 'Item label' })).toHaveClass('opacity-50', 'h-5');
  });

  it('works as the trigger of a Popover', async () => {
    render(
      <Popover>
        <PopoverTrigger asChild>
          <StatusBarItem aria-label='Open panel'>Trigger</StatusBarItem>
        </PopoverTrigger>
        <PopoverContent>Panel body</PopoverContent>
      </Popover>,
    );
    const button = screen.getByRole('button', { name: 'Open panel' });
    expect(button).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(button);
    expect(await screen.findByText('Panel body')).toBeInTheDocument();
    expect(button).toHaveAttribute('aria-expanded', 'true');
  });
});

describe('StatusBarCount', () => {
  it('shows its content in a pill', () => {
    render(<StatusBarCount>3</StatusBarCount>);
    expect(screen.getByText('3')).toHaveClass('rounded-full', 'bg-muted', 'text-2xs');
  });
});
