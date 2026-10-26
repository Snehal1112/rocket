import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncHeader } from '../SpecSyncHeader';
import { makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, checkOpenApiSyncNow: vi.fn() };
});

describe('SpecSyncHeader', () => {
  it('shows the location, last check time and interval', () => {
    renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
    expect(screen.getByText('https://example.com/openapi.json')).toBeInTheDocument();
    expect(screen.getByText('Last checked 2h ago')).toBeInTheDocument();
    expect(screen.getByText('Check interval: Every day')).toBeInTheDocument();
  });

  it('has a Check now button', () => {
    renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
    expect(screen.getByRole('button', { name: 'Check now' })).toBeEnabled();
  });

  it('disables Check now without consent', () => {
    renderWithQuery(
      <SpecSyncHeader collection='c' status={makeStatus({ consent: 'missing' })} />,
    );
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDisabled();
  });

  it('says so when it was never checked', () => {
    renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus({ lastCheckedAt: null })} />);
    expect(screen.getByText('Not checked yet')).toBeInTheDocument();
  });

  it('shows the message of a failed check in an alert', () => {
    renderWithQuery(
      <SpecSyncHeader
        collection='c'
        status={makeStatus({
          lastStatus: { state: 'failed', code: 'timeout' },
          lastMessage: 'The spec host did not answer in time.',
        })}
      />,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('The spec host did not answer in time.');
  });

  it('hides the query of an Https link behind Show full link', async () => {
    renderWithQuery(
      <SpecSyncHeader
        collection='c'
        status={makeStatus({ location: 'https://api.example.com/v1/spec.json?token=abc' })}
      />,
    );
    expect(screen.getByText('api.example.com/v1/spec.json')).toBeInTheDocument();
    expect(screen.queryByText(/token=abc/)).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText('https://api.example.com/v1/spec.json?token=abc')).toBeInTheDocument();
  });
});
