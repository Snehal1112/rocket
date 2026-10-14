import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SavedPill } from '@/components/ui/saved-pill';
import { SecretField } from '@/components/ui/secret-field';

function Controlled(props: { hasSaved: boolean; initialRemoved?: boolean; initialValue?: string }) {
  const [value, setValue] = useState(props.initialValue ?? '');
  const [removed, setRemoved] = useState(props.initialRemoved ?? false);
  return (
    <SecretField
      id='cred'
      value={value}
      onValueChange={setValue}
      hasSaved={props.hasSaved}
      markedForRemoval={removed}
      onToggleRemove={() => setRemoved((current) => !current)}
    />
  );
}

describe('SavedPill', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('still hides itself after 3 seconds by default', () => {
    vi.useFakeTimers();
    render(<SavedPill />);
    expect(screen.getByText('All changes saved')).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.queryByText('All changes saved')).toBeNull();
  });

  it('stays visible and shows a custom label when persistent', () => {
    vi.useFakeTimers();
    render(<SavedPill persistent>Saved in the keychain</SavedPill>);
    act(() => {
      vi.advanceTimersByTime(10000);
    });
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
  });
});

describe('SecretField', () => {
  it('shows a password input when nothing is saved', () => {
    render(<Controlled hasSaved={false} />);
    const input = screen.getByPlaceholderText('Paste the token');
    expect(input).toHaveAttribute('type', 'password');
    expect(input).toHaveAttribute('id', 'cred');
    expect(screen.queryByRole('button', { name: 'Replace' })).toBeNull();
  });

  it('shows the saved pill with Replace and Remove text buttons', () => {
    render(<Controlled hasSaved />);
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Replace' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove' })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText('Paste the token')).toBeNull();
  });

  it('shows the input after Replace and returns on Cancel', async () => {
    render(<Controlled hasSaved />);
    await userEvent.click(screen.getByRole('button', { name: 'Replace' }));
    const input = screen.getByPlaceholderText('Paste the token');
    expect(input).toHaveFocus();
    await userEvent.type(input, 'secret');
    expect(input).toHaveValue('secret');
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByPlaceholderText('Paste the token')).toBeNull();
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
  });

  it('marks the credential for removal and can undo', async () => {
    render(<Controlled hasSaved />);
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(
      screen.getByText('The saved credential will be removed when you save.'),
    ).toBeInTheDocument();
    expect(screen.queryByText('Saved in the keychain')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
  });

  it('keeps the input visible when a value is present', () => {
    render(
      <SecretField
        id='cred'
        value='typed'
        onValueChange={() => {}}
        hasSaved
        markedForRemoval={false}
        onToggleRemove={() => {}}
      />,
    );
    // A non-empty value keeps the input visible, so Replace and Remove are not shown.
    expect(screen.getByDisplayValue('typed')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove' })).toBeNull();
  });

  it('calls onValueChange with an empty string and toggles removal on Remove', async () => {
    const onValueChange = vi.fn();
    const onToggleRemove = vi.fn();
    render(
      <SecretField
        id='cred'
        value=''
        onValueChange={onValueChange}
        hasSaved
        markedForRemoval={false}
        onToggleRemove={onToggleRemove}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(onValueChange).toHaveBeenCalledWith('');
    expect(onToggleRemove).toHaveBeenCalledTimes(1);
  });

  it('forwards aria props and a custom placeholder to the input', () => {
    render(
      <SecretField
        id='cred'
        value=''
        onValueChange={() => {}}
        hasSaved={false}
        markedForRemoval={false}
        onToggleRemove={() => {}}
        placeholder='Paste a key'
        aria-invalid
        aria-describedby='cred-help cred-error'
      />,
    );
    const input = screen.getByPlaceholderText('Paste a key');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'cred-help cred-error');
  });

  it('does not use an icon-only button', () => {
    render(<Controlled hasSaved />);
    for (const button of screen.getAllByRole('button')) {
      expect(button.textContent?.trim()).not.toBe('');
    }
  });
});
