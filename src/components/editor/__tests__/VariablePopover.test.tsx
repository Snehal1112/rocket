import { render, screen } from '@testing-library/react';
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
