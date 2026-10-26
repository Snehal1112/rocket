import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { OpenApiUnchanged } from '@/lib/tauri-api';
import { SpecSyncOverview } from '../SpecSyncOverview';
import { changedItem, makePlan, makeStatus, newItem, removedItem } from './fixtures';

const takeOnly = {
  ...changedItem,
  key: 'PUT /pets',
  changes: [{ field: 'url', outcome: 'takeSpec' as const }],
};

describe('SpecSyncOverview', () => {
  it('counts unchanged, locally edited and pending items', () => {
    const plan = makePlan({
      items: [newItem, removedItem, changedItem, takeOnly],
      unchanged: [{ key: 'a' }, { key: 'b' }] as unknown as OpenApiUnchanged[],
    });
    render(<SpecSyncOverview plan={plan} status={makeStatus()} />);
    expect(screen.getByTestId('overview-in-sync')).toHaveTextContent('2');
    expect(screen.getByTestId('overview-edited')).toHaveTextContent('1');
    expect(screen.getByTestId('overview-pending')).toHaveTextContent('4');
    expect(screen.getByText('Endpoints in sync')).toBeInTheDocument();
    expect(screen.getByText('Changed in your collection')).toBeInTheDocument();
    expect(screen.getByText('Spec updates pending')).toBeInTheDocument();
  });

  it('shows the last checked time, or that it was never checked', () => {
    const view = render(<SpecSyncOverview plan={makePlan()} status={makeStatus()} />);
    expect(screen.getByText('Last checked 2h ago')).toBeInTheDocument();
    view.rerender(
      <SpecSyncOverview plan={makePlan()} status={makeStatus({ lastCheckedAt: null })} />,
    );
    expect(screen.getByText('Not checked yet')).toBeInTheDocument();
  });

  it('counts variable changes as pending updates', () => {
    const plan = makePlan({
      items: [],
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl' }] as never,
    });
    render(<SpecSyncOverview plan={plan} status={makeStatus()} />);
    expect(screen.getByTestId('overview-pending')).toHaveTextContent('1');
  });
});
