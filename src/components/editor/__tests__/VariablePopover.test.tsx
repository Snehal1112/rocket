import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { VariablePopover } from '../VariablePopover';

const SECRET = 'sk-live-do-not-show-123';

const entry = (over: Partial<VariableScopeEntry> = {}): VariableScopeEntry => ({
  value: SECRET,
  source: 'environment',
  label: 'dev',
  secret: false,
  ...over,
});

function renderPopover(e: VariableScopeEntry) {
  return render(
    <VariablePopover
      varName='apiKey'
      entry={e}
      tokenType='variable'
      onCommit={vi.fn(async () => undefined)}
      onClose={vi.fn()}
    />,
  );
}

describe('VariablePopover secret handling', () => {
  it('shows the value of a non-secret variable', () => {
    renderPopover(entry());
    expect(screen.getByRole('textbox')).toHaveValue(SECRET);
  });

  it('masks a secret variable and keeps the value out of the whole document', () => {
    renderPopover(entry({ secret: true }));
    const input = screen.getByRole('textbox');
    expect(input).toHaveValue('●●●●');
    expect(input).toHaveAttribute('readonly');
    expect(document.body.innerHTML).not.toContain(SECRET);
  });
});

describe('VariablePopover readOnly prop', () => {
  it('is editable for an environment variable by default', () => {
    renderPopover(entry());
    expect(screen.getByRole('textbox')).not.toHaveAttribute('readonly');
  });

  it('is read-only when the editor cannot save the change', async () => {
    const onCommit = vi.fn(async () => undefined);
    render(
      <VariablePopover
        varName='apiKey'
        entry={entry()}
        tokenType='variable'
        readOnly
        onCommit={onCommit}
        onClose={vi.fn()}
      />,
    );
    const input = screen.getByRole('textbox');
    expect(input).toHaveAttribute('readonly');
    await userEvent.type(input, 'x{Enter}');
    expect(onCommit).not.toHaveBeenCalled();
  });
});

describe('VariablePopover blur', () => {
  it('closes a forced read-only popover on blur without committing', async () => {
    const onCommit = vi.fn(async () => undefined);
    const onClose = vi.fn();
    render(
      <VariablePopover
        varName='apiKey'
        entry={entry()}
        tokenType='variable'
        readOnly
        onCommit={onCommit}
        onClose={onClose}
      />,
    );
    const input = screen.getByRole('textbox');
    input.focus();
    input.blur();
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('still commits an editable popover on blur', async () => {
    const onCommit = vi.fn(async () => undefined);
    const onClose = vi.fn();
    render(
      <VariablePopover
        varName='apiKey'
        entry={entry()}
        tokenType='variable'
        onCommit={onCommit}
        onClose={onClose}
      />,
    );
    const input = screen.getByRole('textbox');
    input.focus();
    input.blur();
    await vi.waitFor(() => expect(onCommit).toHaveBeenCalledWith(SECRET));
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});

describe('VariablePopover empty secret hint', () => {
  const HINT = /environment editor/i;

  it('hints to open the environment editor when an environment secret is empty', () => {
    renderPopover(entry({ secret: true, value: '' }));
    expect(screen.getByText(HINT)).toBeInTheDocument();
  });

  it('shows the same hint for an empty global secret', () => {
    renderPopover(entry({ secret: true, value: '', source: 'global', label: 'Global' }));
    expect(screen.getByText(HINT)).toBeInTheDocument();
  });

  it('shows no hint when the secret has a value', () => {
    renderPopover(entry({ secret: true }));
    expect(screen.queryByText(HINT)).toBeNull();
  });

  it('shows no hint for a vault secret, which is always empty here', () => {
    renderPopover(entry({ secret: true, value: '', source: 'vault', label: 'Vault (pay)' }));
    expect(screen.queryByText(HINT)).toBeNull();
  });

  it('shows no hint for a process env secret', () => {
    renderPopover(entry({ secret: true, value: '', source: 'process', label: 'Process Env' }));
    expect(screen.queryByText(HINT)).toBeNull();
  });

  it('keeps the empty secret read-only', () => {
    renderPopover(entry({ secret: true, value: '' }));
    expect(screen.getByRole('textbox')).toHaveAttribute('readonly');
  });
});
