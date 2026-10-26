import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { OpenApiApplyReport } from '@/lib/tauri-api';
import { ApplyResultDialog } from '../ApplyResultDialog';

const report: OpenApiApplyReport = {
  applied: [
    { key: 'GET /a', kind: 'created', requestPath: 'pets/a.yml' },
    { key: 'GET /b', kind: 'updated', requestPath: 'pets/b.yml' },
  ],
  stale: ['GET /s'],
  notRewritable: ['GET /n'],
  deleted: ['GET /d'],
  detached: ['GET /t'],
  skipped: 2,
  remaining: 4,
};

function renderDialog(r: OpenApiApplyReport, onCheckAgain = vi.fn(), checking = false) {
  render(
    <ApplyResultDialog report={r} onClose={vi.fn()} onCheckAgain={onCheckAgain} checking={checking} />,
  );
  return onCheckAgain;
}

describe('ApplyResultDialog', () => {
  it('lists each group in plain words', () => {
    renderDialog(report);
    expect(screen.getByText('2 applied: 1 created, 1 updated.')).toBeInTheDocument();
    expect(screen.getByText('pets/a.yml')).toBeInTheDocument();
    expect(
      screen.getByText(/1 not applied because the spec changed since the list was made/),
    ).toBeInTheDocument();
    expect(screen.getByText('GET /s')).toBeInTheDocument();
    expect(
      screen.getByText(/1 left as it is because Rocket cannot rewrite its file/),
    ).toBeInTheDocument();
    expect(screen.getByText('1 deleted because the spec removed it.')).toBeInTheDocument();
    expect(
      screen.getByText('1 removed from the spec but kept in your collection, no longer tracked.'),
    ).toBeInTheDocument();
    expect(screen.getByText('2 skipped.')).toBeInTheDocument();
    expect(screen.getByText('4 changes still waiting.')).toBeInTheDocument();
  });

  it('claims nothing for an empty applied list and leaves out empty groups', () => {
    renderDialog({
      applied: [],
      stale: [],
      notRewritable: [],
      deleted: [],
      detached: [],
      skipped: 0,
      remaining: 0,
    });
    expect(screen.getByText('Nothing was applied.')).toBeInTheDocument();
    expect(screen.queryByText(/deleted/)).toBeNull();
    expect(screen.queryByText(/skipped/)).toBeNull();
    expect(screen.queryByText(/still waiting/)).toBeNull();
  });

  it('runs Check again', async () => {
    const onCheckAgain = renderDialog(report);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Check again' }));
    expect(onCheckAgain).toHaveBeenCalledTimes(1);
  });

  it('disables Check again while it runs', () => {
    renderDialog(report, vi.fn(), true);
    expect(screen.getByRole('button', { name: 'Check again' })).toBeDisabled();
  });
});
