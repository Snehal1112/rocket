import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';

function Demo(props: {
  id?: string;
  error?: string | null;
  help?: string;
  optionalHint?: string;
}) {
  return (
    <FormField
      id={props.id ?? 'demo'}
      label='Idle timeout'
      optionalHint={props.optionalHint}
      help={props.help ?? 'Seconds before an idle session stops.'}
      error={props.error}
    >
      {(a11y) => <Input {...a11y} />}
    </FormField>
  );
}

describe('FormField', () => {
  it('wires the label, the control and the help text together', () => {
    render(<Demo />);
    const input = screen.getByLabelText(/Idle timeout/);
    expect(input).toHaveAttribute('id', 'demo');
    expect(input).toHaveAttribute('aria-invalid', 'false');
    expect(input).toHaveAttribute('aria-describedby', 'demo-help');
    const help = screen.getByText('Seconds before an idle session stops.');
    expect(help).toHaveAttribute('id', 'demo-help');
    expect(help).toHaveClass('text-xs', 'text-muted-foreground');
  });

  it('shows the error with role alert and links it to the control', () => {
    render(<Demo error='Enter a value between 30 and 1800.' />);
    const input = screen.getByLabelText(/Idle timeout/);
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'demo-help demo-error');
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Enter a value between 30 and 1800.');
    expect(alert).toHaveAttribute('id', 'demo-error');
    expect(alert).toHaveClass('text-destructive');
  });

  it('renders no alert for a null or empty error', () => {
    const { rerender } = render(<Demo error={null} />);
    expect(screen.queryByRole('alert')).toBeNull();
    rerender(<Demo error='' />);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByLabelText(/Idle timeout/)).toHaveAttribute('aria-invalid', 'false');
  });

  it('shows the optional hint beside the label', () => {
    render(<Demo optionalHint='seconds, optional' />);
    expect(screen.getByText('(seconds, optional)')).toBeInTheDocument();
  });

  it('wraps a very long unbroken help string', () => {
    const long = 'x'.repeat(400);
    render(<Demo help={long} />);
    const help = screen.getByText(long);
    expect(help).toHaveClass('break-words');
    expect(help).not.toHaveClass('truncate');
  });

  it('never emits duplicate ids', () => {
    const { container } = render(
      <>
        <Demo id='one' error='Bad.' />
        <Demo id='two' error='Bad.' />
      </>,
    );
    const ids = Array.from(container.querySelectorAll('[id]')).map((node) => node.id);
    expect(ids.length).toBe(6);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('associates the label with the control through htmlFor in normal mode', () => {
    const { container } = render(<Demo />);
    const label = container.querySelector('label');
    expect(label).not.toBeNull();
    expect(label).toHaveAttribute('for', 'demo');
    expect(screen.queryByRole('group')).toBeNull();
  });

  describe('group mode', () => {
    function GroupDemo(props: { error?: string | null }) {
      return (
        <FormField
          group
          id='grp'
          label='Saved credential'
          help='Stored in the keychain.'
          error={props.error}
        >
          {(a11y) => (
            <div data-testid='child' {...a11y}>
              content
            </div>
          )}
        </FormField>
      );
    }

    it('renders role group named by the label', () => {
      const { container } = render(<GroupDemo />);
      const group = screen.getByRole('group', { name: 'Saved credential' });
      const label = container.querySelector('#grp-label');
      expect(label).not.toBeNull();
      expect(group).toHaveAttribute('aria-labelledby', 'grp-label');
      expect(label?.tagName).toBe('SPAN');
      expect(container.querySelector('label')).toBeNull();
    });

    it('still passes id, aria-invalid and aria-describedby to the child', () => {
      render(<GroupDemo error='Bad.' />);
      const child = screen.getByTestId('child');
      expect(child).toHaveAttribute('id', 'grp');
      expect(child).toHaveAttribute('aria-invalid', 'true');
      expect(child).toHaveAttribute('aria-describedby', 'grp-help grp-error');
    });

    it('never emits duplicate ids', () => {
      const { container } = render(<GroupDemo error='Bad.' />);
      const ids = Array.from(container.querySelectorAll('[id]')).map((node) => node.id);
      expect(new Set(ids).size).toBe(ids.length);
    });
  });
});
