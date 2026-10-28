import { beforeEach, describe, expect, it } from 'vitest';
import { collectAllTabs } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { closeDeletedRequestTabs } from '../closeDeletedRequestTabs';

function open(id: string, collection: string, path: string, isDirty = false) {
  usePaneStore.getState().openTab({
    id,
    title: id,
    tabType: 'request',
    request: {} as never,
    response: null,
    isDirty,
    source: { collection, path },
  } as never);
}

function ids(): string[] {
  return collectAllTabs(usePaneStore.getState().root).map((t) => t.id);
}

describe('closeDeletedRequestTabs', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('closes_the_tab_of_a_deleted_request_even_when_it_has_unsaved_edits', () => {
    open('a', 'c', 'pets/x', true);
    open('b', 'c', 'pets/y');
    closeDeletedRequestTabs('c', ['pets/x']);
    expect(ids()).toEqual(['b']);
  });

  it('matches_the_path_with_its_file_extension', () => {
    open('a', 'c', 'pets/x.yml');
    open('b', 'c', 'pets/y.yaml');
    closeDeletedRequestTabs('c', ['pets/x', 'pets/y']);
    expect(ids()).toEqual([]);
  });

  it('leaves_other_collections_alone', () => {
    open('a', 'other', 'pets/x');
    closeDeletedRequestTabs('c', ['pets/x']);
    expect(ids()).toEqual(['a']);
  });
});
