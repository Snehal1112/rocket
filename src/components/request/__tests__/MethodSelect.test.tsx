import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MethodSelect } from '../MethodSelect';

describe('MethodSelect', () => {
  it('shows a custom method that is not in the standard list', () => {
    render(<MethodSelect value='PURGE' onChange={vi.fn()} />);
    expect(screen.getByRole('combobox')).toHaveTextContent('PURGE');
  });

  it('commits a valid custom method typed into the custom field', () => {
    const onChange = vi.fn();
    render(<MethodSelect value='GET' onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Custom method' }));
    const input = screen.getByLabelText('Custom HTTP method');
    fireEvent.change(input, { target: { value: 'PURGE' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('PURGE');
  });

  it('does not commit text that is not a method token', () => {
    const onChange = vi.fn();
    render(<MethodSelect value='GET' onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Custom method' }));
    const input = screen.getByLabelText('Custom HTTP method');
    fireEvent.change(input, { target: { value: 'not valid' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).not.toHaveBeenCalled();
    expect(input).toHaveAttribute('aria-invalid', 'true');
  });
});
