import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CollectionTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';
import { useSettingsStore } from '../settings-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn(), cancelAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const collectionTab: CollectionTab = {
  id: 'collection-demo',
  title: 'demo',
  isDirty: false,
  tabType: 'collection',
  collectionName: 'demo',
};

describe('pane store and Settings', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useSettingsStore.getState().reset();
  });

  it('closes Settings when a tab opens', () => {
    useSettingsStore.getState().openSettings();
    usePaneStore.getState().openEphemeralTab();
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('keeps Settings open when a tab opens while locked, and still opens the tab', () => {
    useSettingsStore.getState().openSettings();
    useSettingsStore.getState().setLocked(true);
    usePaneStore.getState().openEphemeralTab();
    expect(useSettingsStore.getState().open).toBe(true);
    const { root } = usePaneStore.getState();
    expect(root.type === 'leaf' && root.tabs.some((t) => t.title === 'Untitled')).toBe(true);
  });

  it('closes Settings when an open collection tab is focused', () => {
    usePaneStore.getState().openTab(collectionTab);
    useSettingsStore.getState().openSettings();
    expect(usePaneStore.getState().openCollectionTab('demo', 'overview')).toBe(true);
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('keeps Settings open when no collection tab is found', () => {
    useSettingsStore.getState().openSettings();
    expect(usePaneStore.getState().openCollectionTab('missing', 'overview')).toBe(false);
    expect(useSettingsStore.getState().open).toBe(true);
  });
});
