import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncConsentBanner } from '../SpecSyncConsentBanner';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, grantOpenApiSyncConsent: vi.fn() };
});

const LOCATION = 'https://example.com/openapi.json?key=abc';

describe('SpecSyncConsentBanner', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('explains a missing approval and shows the exact location', () => {
    renderWithQuery(<SpecSyncConsentBanner collection='c' location={LOCATION} consent='missing' />);
    expect(screen.getByText('Trust this spec link')).toBeInTheDocument();
    expect(screen.getByText(LOCATION)).toBeInTheDocument();
    expect(
      screen.getByText(
        'This collection came from another place or was moved. Rocket does not contact the link until you approve it on this machine.',
      ),
    ).toBeInTheDocument();
  });

  it('explains a changed location', () => {
    renderWithQuery(
      <SpecSyncConsentBanner collection='c' location={LOCATION} consent='locationChanged' />,
    );
    expect(
      screen.getByText(
        'The spec link in this collection changed. Approve the new link before Rocket contacts it.',
      ),
    ).toBeInTheDocument();
  });

  it('approves with the same text it displays, once on a double click', async () => {
    vi.mocked(tauriApi.grantOpenApiSyncConsent).mockImplementation(
      () => new Promise((r) => setTimeout(() => r(undefined), 30)),
    );
    renderWithQuery(<SpecSyncConsentBanner collection='c' location={LOCATION} consent='missing' />);
    await userEvent.setup().dblClick(screen.getByRole('button', { name: 'Approve' }));
    expect(tauriApi.grantOpenApiSyncConsent).toHaveBeenCalledTimes(1);
    expect(tauriApi.grantOpenApiSyncConsent).toHaveBeenCalledWith('c', LOCATION);
  });

  it('shows a clear sentence when the link cannot be approved', async () => {
    vi.mocked(tauriApi.grantOpenApiSyncConsent).mockRejectedValue(
      'This spec location cannot be approved because its link is ambiguous.',
    );
    renderWithQuery(<SpecSyncConsentBanner collection='c' location={LOCATION} consent='missing' />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Approve' }));
    expect(
      await screen.findByText(/cannot be approved because it is ambiguous/),
    ).toBeInTheDocument();
    expect(screen.getByText(/branch name with a slash/)).toBeInTheDocument();
  });
});
