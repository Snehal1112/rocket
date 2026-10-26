import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SpecSyncHeader } from '../SpecSyncHeader';
import { makeStatus } from './fixtures';

describe('SpecSyncHeader', () => {
  it('shows the location, last check time and interval', () => {
    render(<SpecSyncHeader status={makeStatus()} />);
    expect(screen.getByText('https://example.com/openapi.json')).toBeInTheDocument();
    expect(screen.getByText('Last checked 2h ago')).toBeInTheDocument();
    expect(screen.getByText('Check interval: Every day')).toBeInTheDocument();
  });

  it('has no Check now button yet', () => {
    render(<SpecSyncHeader status={makeStatus()} />);
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.queryByText(/check now/i)).toBeNull();
  });

  it('says so when it was never checked', () => {
    render(<SpecSyncHeader status={makeStatus({ lastCheckedAt: null })} />);
    expect(screen.getByText('Not checked yet')).toBeInTheDocument();
  });
});
