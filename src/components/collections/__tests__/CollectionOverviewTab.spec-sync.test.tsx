import { screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { makeStatus } from '@/components/collections/spec-sync/__tests__/fixtures';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import type { CollectionSection, CollectionTab } from '@/types/pane-types';
import { CollectionOverviewTab, resolveCollectionSection } from '../CollectionOverviewTab';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn().mockResolvedValue({
      name: 'demo',
      settings: { headers: [], variables: [], docs: '' },
      root: { uid: 'r', name: 'demo', items: [] },
    }),
    getCollectionSettings: vi.fn().mockResolvedValue({ headers: [], variables: [], docs: '' }),
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncPlan: vi.fn().mockResolvedValue({
      pending: false,
      items: [],
      variableChanges: [],
      unchanged: [],
      skippedHidden: 0,
      deletedByUser: [],
      cautious: false,
      warnings: [],
    }),
    onOpenApiSyncChecked: vi.fn(() => Promise.resolve(() => undefined)),
  };
});

vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: () => ({ data: [] }),
  useGlobalEnvironment: () => ({ data: null }),
  useGlobalEnvironmentName: () => ({ data: null }),
  useProcessEnvVars: () => ({ data: {} }),
}));

vi.mock('@/components/collections/CollectionTrustBanner', () => ({
  CollectionTrustBanner: () => null,
}));
vi.mock('@/components/collections/CollectionTrustSection', () => ({
  CollectionTrustSection: () => null,
}));
vi.mock('@/components/collections/MarkdownEditor', () => ({ MarkdownEditor: () => null }));
vi.mock('@/components/request/AuthEditor', () => ({ AuthEditor: () => null }));
vi.mock('@/components/request/HeadersEditor', () => ({ HeadersEditor: () => null }));
vi.mock('../CollectionVariablesEditor', () => ({ CollectionVariablesEditor: () => null }));

function makeTab(activeSection?: CollectionSection): CollectionTab {
  return {
    id: 't1',
    title: 'demo',
    isDirty: false,
    tabType: 'collection',
    collectionName: 'demo',
    activeSection,
  };
}

describe('CollectionOverviewTab Spec sync tab', () => {
  beforeEach(() => vi.clearAllMocks());

  it('has no Spec sync tab when the status is null', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    renderWithQuery(<CollectionOverviewTab tab={makeTab('overview')} />);
    await screen.findByText('Documentation');
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    expect(screen.queryByText('Spec sync')).toBeNull();
  });

  it('shows the Spec sync tab when the status has data', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    renderWithQuery(<CollectionOverviewTab tab={makeTab('overview')} />);
    expect(await screen.findByText('Spec sync')).toBeInTheDocument();
  });

  it('shows the Spec sync tab and its message when the status errored', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue('bad yaml');
    renderWithQuery(<CollectionOverviewTab tab={makeTab('spec-sync')} />);
    expect(await screen.findByText(/The sync state file cannot be read/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Spec sync' })).toBeInTheDocument();
  });

  it('falls back to the overview for a stale spec-sync section', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    renderWithQuery(<CollectionOverviewTab tab={makeTab('spec-sync')} />);
    expect(await screen.findByText('Default Headers')).toBeInTheDocument();
  });
});

describe('resolveCollectionSection', () => {
  const valid: CollectionSection[] = ['overview', 'auth', 'spec-sync'];

  it('keeps spec-sync while the status loads', () => {
    expect(
      resolveCollectionSection('spec-sync', valid, {
        specSyncAvailable: false,
        specSyncPending: true,
      }),
    ).toBe('spec-sync');
  });

  it('falls back once the status is known to be absent', () => {
    expect(
      resolveCollectionSection('spec-sync', valid, {
        specSyncAvailable: false,
        specSyncPending: false,
      }),
    ).toBe('overview');
  });

  it('falls back for an unknown section', () => {
    expect(
      resolveCollectionSection('gone' as CollectionSection, valid, {
        specSyncAvailable: true,
        specSyncPending: false,
      }),
    ).toBe('overview');
  });
});
