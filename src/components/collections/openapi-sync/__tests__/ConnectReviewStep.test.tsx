import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { ConnectReviewStep } from '../ConnectReviewStep';
import { type ConnectRowChoice, defaultRows } from '../connectRows';
import { makeConnectPreview } from './fixtures';

function Harness({ initial }: { initial: ConnectRowChoice[] }) {
  const [rows, setRows] = useState(initial);
  return (
    <ConnectReviewStep
      rows={rows}
      onChange={(id, choice) => setRows((rs) => rs.map((r) => (r.id === id ? { ...r, choice } : r)))}
    />
  );
}

describe('ConnectReviewStep', () => {
  beforeAll(() => {
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
    HTMLElement.prototype.setPointerCapture = vi.fn();
    HTMLElement.prototype.releasePointerCapture = vi.fn();
    HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  it('step_two_groups_rows_into_sections', () => {
    const base = makeConnectPreview();
    const rows = defaultRows(
      makeConnectPreview({
        ambiguous: [{ requests: base.yoursOnly, operations: base.inSpecOnly }],
      }),
    );
    render(<Harness initial={rows} />);
    const headings = screen.getAllByRole('heading').map((h) => h.textContent);
    expect(headings).toEqual([
      'Needs your decision (1)',
      'Matched (1)',
      'Suggested (1)',
      'In spec only (1)',
      'Yours only (1)',
    ]);
    expect(screen.getByText('{{baseUrl}}/pets')).toBeInTheDocument();
  });

  it('changing_a_select_changes_the_summary', async () => {
    const user = userEvent.setup();
    render(<Harness initial={defaultRows(makeConnectPreview())} />);
    expect(screen.getByText('1 linked, 1 added, 1 skipped')).toBeInTheDocument();
    await user.click(screen.getByRole('combobox', { name: 'Choice for GET /pets' }));
    await user.click(await screen.findByRole('option', { name: 'Keep mine' }));
    expect(screen.getByText('0 linked, 1 added, 2 skipped')).toBeInTheDocument();
  });

  it('a_suggested_row_shows_that_it_is_only_a_guess', () => {
    render(<Harness initial={defaultRows(makeConnectPreview())} />);
    expect(screen.getByText(/only a guess/i)).toBeInTheDocument();
  });
});
