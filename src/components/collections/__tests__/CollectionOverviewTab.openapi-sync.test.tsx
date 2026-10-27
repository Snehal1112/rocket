import { act, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { makeStatus } from '@/components/collections/openapi-sync/__tests__/fixtures';
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
    onOpenApiSyncApplied: vi.fn(() => Promise.resolve(() => undefined)),
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

describe('CollectionOverviewTab without the sync section', () => {
  beforeEach(() => vi.clearAllMocks());

  it('has_no_spec_sync_tab_any_more', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    renderWithQuery(<CollectionOverviewTab tab={makeTab('overview')} />);
    await screen.findByText('Documentation');
    for (const label of ['Overview', 'Authorization', 'Variables', 'Documentation']) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
    expect(screen.queryByRole('button', { name: /sync/i })).toBeNull();
  });

  it('a_stored_spec_sync_section_falls_back_to_overview', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    renderWithQuery(<CollectionOverviewTab tab={makeTab('spec-sync' as never)} />);
    expect(await screen.findByText('Default Headers')).toBeInTheDocument();
  });

  it('shows_no_sync_notice', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ hasPending: true, lastStatus: { state: 'changed' } }),
    );
    renderWithQuery(<CollectionOverviewTab tab={makeTab('overview')} />);
    await screen.findByText('Documentation');
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(screen.queryByText('A new version of the spec was found')).toBeNull();
    expect(screen.queryByText(/new version of the spec/i)).toBeNull();
  });
});

describe('resolveCollectionSection', () => {
  const valid: CollectionSection[] = ['overview', 'auth'];

  it('keeps a valid section', () => {
    expect(resolveCollectionSection('auth', valid)).toBe('auth');
  });

  it('falls back for a stored spec-sync section', () => {
    expect(resolveCollectionSection('spec-sync' as never, valid)).toBe('overview');
  });

  it('falls back for an unknown section', () => {
    expect(resolveCollectionSection('gone' as CollectionSection, valid)).toBe('overview');
  });
});
