import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import type { CollectionTab } from '@/types/pane-types';
import { CollectionOverviewTab } from '../CollectionOverviewTab';

function settingsWith(runnerDataFile?: string) {
  return {
    headers: [],
    variables: [],
    docs: '',
    sandboxMode: 'safe',
    scriptContextRoots: ['../shared'],
    scriptFlow: 'sequential',
    agentAutonomyEnabled: true,
    ...(runnerDataFile ? { runnerDataFile } : {}),
  };
}

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    getCollectionSettings: vi.fn(),
    saveCollectionSettings: vi.fn().mockResolvedValue(undefined),
    getOpenApiSyncStatus: vi.fn().mockResolvedValue(null),
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

const tab: CollectionTab = {
  id: 't1',
  title: 'demo',
  isDirty: false,
  tabType: 'collection',
  collectionName: 'demo',
  activeSection: 'overview',
};

function setup(saved?: string) {
  vi.mocked(tauriApi.getCollection).mockResolvedValue({
    name: 'demo',
    settings: settingsWith(saved),
    root: { uid: 'r', name: 'demo', items: [] },
  } as never);
  vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(settingsWith(saved) as never);
  return renderWithQuery(<CollectionOverviewTab tab={tab} />);
}

async function field() {
  return (await screen.findByLabelText('Runner data file')) as HTMLInputElement;
}

function lastSaved() {
  const calls = vi.mocked(tauriApi.saveCollectionSettings).mock.calls;
  return calls[calls.length - 1][1];
}

async function clickSave() {
  const buttons = await screen.findAllByRole('button', { name: 'Save' });
  const enabled = buttons.find((b) => !(b as HTMLButtonElement).disabled);
  expect(enabled).toBeDefined();
  fireEvent.click(enabled as HTMLElement);
}

describe('Runner data file field', () => {
  beforeEach(() => vi.clearAllMocks());

  it('shows_the_saved_runner_data_file', async () => {
    setup('data/users.csv');
    expect((await field()).value).toBe('data/users.csv');
  });

  it('editing_the_field_saves_a_trimmed_relative_path', async () => {
    setup();
    fireEvent.change(await field(), { target: { value: '  data/users.csv  ' } });
    await clickSave();
    await waitFor(() => expect(tauriApi.saveCollectionSettings).toHaveBeenCalled());
    const saved = lastSaved();
    expect(saved.runnerDataFile).toBe('data/users.csv');
    expect(saved.sandboxMode).toBe('safe');
    expect(saved.scriptContextRoots).toEqual(['../shared']);
    expect(saved.scriptFlow).toBe('sequential');
    expect(saved.agentAutonomyEnabled).toBe(true);
  });

  it('clearing_the_field_saves_no_path', async () => {
    setup('data/users.csv');
    fireEvent.change(await field(), { target: { value: '   ' } });
    await clickSave();
    await waitFor(() => expect(tauriApi.saveCollectionSettings).toHaveBeenCalled());
    const saved = lastSaved();
    expect(saved.runnerDataFile).toBeUndefined();
    expect(JSON.parse(JSON.stringify(saved))).not.toHaveProperty('runnerDataFile');
  });

  it.each([
    '/etc/users.csv',
    '../up/users.csv',
    'a\\b.csv',
    'C:users.csv',
  ])('the_field_never_blocks_saving_when_the_path_looks_invalid (%s)', async (bad) => {
    setup();
    fireEvent.change(await field(), { target: { value: bad } });
    expect(screen.getByText(/does not look like a relative path/i)).toBeInTheDocument();
    await clickSave();
    await waitFor(() => expect(tauriApi.saveCollectionSettings).toHaveBeenCalled());
    expect(lastSaved().runnerDataFile).toBe(bad);
  });

  it('the_hint_explains_the_path_is_relative_to_the_collection', async () => {
    setup();
    await field();
    expect(screen.getByText(/relative to the collection folder/i)).toBeInTheDocument();
  });

  it('the_input_is_described_by_the_hint', async () => {
    setup();
    const input = await field();
    const text = describedText(input);
    expect(text).toMatch(/relative to the collection folder/i);
    expect(text).not.toMatch(/does not look like a relative path/i);
  });

  it('the_input_is_described_by_the_warning_when_the_path_looks_invalid', async () => {
    setup();
    const input = await field();
    fireEvent.change(input, { target: { value: '../up/users.csv' } });
    const text = describedText(input);
    expect(text).toMatch(/relative to the collection folder/i);
    expect(text).toMatch(/does not look like a relative path/i);
    expect(screen.getByRole('status')).toHaveTextContent(/does not look like a relative path/i);
  });
});

function describedText(el: HTMLElement): string {
  const ids = (el.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean);
  return ids.map((id) => document.getElementById(id)?.textContent ?? '').join(' ');
}
