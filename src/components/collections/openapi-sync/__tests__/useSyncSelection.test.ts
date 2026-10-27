import { act, renderHook } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import type { OpenApiSyncItem, OpenApiSyncPlan } from '@/lib/tauri-api';
import { useSyncSelection } from '../useSyncSelection';
import { changedItem, makePlan, newItem, removedItem } from './fixtures';

type Links = Array<[string, string]>;

function setup(plan: OpenApiSyncPlan | undefined = makePlan()) {
  return renderHook(
    ({ p }) => {
      const [links, setLinks] = useState<Links>([]);
      return useSyncSelection(p, links, setLinks);
    },
    { initialProps: { p: plan as OpenApiSyncPlan | undefined } },
  );
}

describe('useSyncSelection', () => {
  it('toggles an approval', () => {
    const { result } = setup();
    act(() => result.current.toggle('DELETE /pets'));
    expect(result.current.approved.has('DELETE /pets')).toBe(true);
    expect(result.current.selection.items.map((i) => i.key)).toContain('DELETE /pets');
    act(() => result.current.toggle('DELETE /pets'));
    expect(result.current.approved.has('DELETE /pets')).toBe(false);
  });

  it('selects many keys, including ones the list does not show', () => {
    const items: OpenApiSyncItem[] = Array.from({ length: 700 }, (_, i) => ({
      ...changedItem,
      key: `GET /p${i}`,
      name: `Request ${i}`,
    })) as OpenApiSyncItem[];
    const { result } = setup(makePlan({ items }));
    act(() =>
      result.current.setMany(
        items.map((i) => i.key),
        true,
      ),
    );
    expect(result.current.approved.size).toBe(700);
    act(() => result.current.setMany(['GET /p1', 'GET /p2'], false));
    expect(result.current.approved.size).toBe(698);
  });

  it('keeps an item from being both approved and skipped', () => {
    const { result } = setup();
    act(() => result.current.toggle('POST /pets'));
    act(() => result.current.skip(['POST /pets']));
    expect(result.current.approved.has('POST /pets')).toBe(false);
    expect(result.current.skips.has('POST /pets')).toBe(true);
    act(() => result.current.toggle('POST /pets'));
    expect(result.current.skips.has('POST /pets')).toBe(false);
    expect(result.current.approved.has('POST /pets')).toBe(true);
  });

  it('builds a selection with fingerprints for skips', () => {
    const { result } = setup();
    act(() => result.current.skip(['POST /pets']));
    expect(result.current.selection.skips).toEqual([{ key: 'POST /pets', fingerprint: 'f2' }]);
  });

  it('records field choices and drops them when the item is not approved', () => {
    const { result } = setup();
    act(() => result.current.setChoice('POST /pets', 'url', 'spec'));
    expect(result.current.selection.items.map((i) => i.key)).toEqual(['GET /pets']);
    act(() => result.current.toggle('POST /pets'));
    expect(result.current.selection.items.find((i) => i.key === 'POST /pets')?.choices).toEqual({
      url: 'spec',
    });
  });

  it('approves variables on their own', () => {
    const plan = makePlan({
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
    });
    const { result } = setup(plan);
    expect(result.current.selection.variables).toEqual([{ name: 'baseUrl', digest: 'dv' }]);
    act(() => result.current.toggleVariable('baseUrl'));
    expect(result.current.selection.variables).toEqual([]);
    act(() => result.current.toggleVariable('baseUrl'));
    expect(result.current.selection.variables).toEqual([{ name: 'baseUrl', digest: 'dv' }]);
  });

  it('links and unlinks a rename', () => {
    const { result } = setup();
    act(() => result.current.linkRename('DELETE /pets', 'GET /pets'));
    act(() => result.current.linkRename('DELETE /pets', 'GET /pets'));
    expect(result.current.links).toEqual([['DELETE /pets', 'GET /pets']]);
    expect(result.current.selection.renameLinks).toEqual([['DELETE /pets', 'GET /pets']]);
    act(() => result.current.unlink('DELETE /pets'));
    expect(result.current.links).toEqual([]);
  });

  it('clears the selection when the plan has a new digest set, not when it is the same', () => {
    const { result, rerender } = setup();
    act(() => result.current.toggle('POST /pets'));
    rerender({ p: makePlan() });
    expect(result.current.approved.has('POST /pets')).toBe(true);
    rerender({ p: makePlan({ items: [removedItem, { ...changedItem, digest: 'new' }, newItem] }) });
    // The manual approval is gone and only the defaults remain.
    expect(result.current.approved.has('POST /pets')).toBe(false);
    expect([...result.current.approved]).toEqual(['GET /pets']);
  });

  it('reset clears the selection but keeps the links', () => {
    const { result } = setup();
    act(() => result.current.toggle('POST /pets'));
    expect(result.current.approved.size).toBe(2);
    act(() => result.current.linkRename('a', 'b'));
    act(() => result.current.reset());
    expect(result.current.approved.size).toBe(0);
    expect(result.current.links).toEqual([['a', 'b']]);
  });

  it('keeps a link through new plans while the rename hint or the merged item is there', () => {
    const { result, rerender } = setup();
    act(() => result.current.linkRename('GET /animals', 'GET /pets'));
    // A background refresh: the unlinked plan still has the rename hint.
    rerender({ p: makePlan({ items: [{ ...newItem, digest: 'again' }] }) });
    expect(result.current.links).toEqual([['GET /animals', 'GET /pets']]);
    // The linked plan: the pair is one Changed item under the new key.
    rerender({
      p: makePlan({ items: [{ ...changedItem, key: 'GET /pets', digest: 'linked' }] }),
    });
    expect(result.current.links).toEqual([['GET /animals', 'GET /pets']]);
    rerender({
      p: makePlan({ items: [{ ...changedItem, key: 'GET /pets', digest: 'later' }] }),
    });
    expect(result.current.links).toEqual([['GET /animals', 'GET /pets']]);
  });

  it('keeps a pure-rename link whose old key is listed as unchanged', () => {
    const { result, rerender } = setup();
    act(() => result.current.linkRename('GET /animals', 'GET /pets'));
    // A rename with no content change merges to nothing and shows under unchanged.
    rerender({
      p: makePlan({
        items: [{ ...changedItem, key: 'POST /other', digest: 'o' }],
        unchanged: [{ key: 'GET /animals', name: 'List pets' }],
      }),
    });
    expect(result.current.links).toEqual([['GET /animals', 'GET /pets']]);
  });

  it('drops a link whose new key and hint are gone from the plan', () => {
    const { result, rerender } = setup();
    act(() => result.current.linkRename('GET /animals', 'GET /pets'));
    act(() => result.current.linkRename('x', 'y'));
    rerender({ p: makePlan({ items: [{ ...newItem, digest: 'again' }] }) });
    expect(result.current.links).toEqual([['GET /animals', 'GET /pets']]);
    rerender({ p: makePlan({ items: [{ ...changedItem, key: 'POST /other', digest: 'o' }] }) });
    expect(result.current.links).toEqual([]);
  });

  it('clearLinks drops every link', () => {
    const { result } = setup();
    act(() => result.current.linkRename('a', 'b'));
    act(() => result.current.clearLinks());
    expect(result.current.links).toEqual([]);
    expect(result.current.selection.renameLinks).toEqual([]);
  });

  it('keeps the links when a plan arrives after an error under the same links', () => {
    const { result, rerender } = setup();
    act(() => result.current.linkRename('GET /animals', 'GET /pets'));
    rerender({ p: makePlan({ items: [{ ...changedItem, key: 'GET /pets', digest: 'linked' }] }) });
    // The plan query fails, then Check again succeeds under the same links.
    rerender({ p: undefined });
    rerender({ p: makePlan({ items: [{ ...changedItem, key: 'GET /pets', digest: 'again' }] }) });
    expect(result.current.links).toEqual([['GET /animals', 'GET /pets']]);
  });
});

const editedRemoved = { ...removedItem, key: 'DELETE /edited', digest: 'd4', fingerprint: 'f4', edited: true } as OpenApiSyncItem;
const cleanRemoved = { ...removedItem, edited: false } as OpenApiSyncItem;
const plainChanged = {
  ...changedItem,
  key: 'PUT /pets',
  digest: 'd5',
  fingerprint: 'f5',
  changes: [{ field: 'header:Accept', outcome: 'takeSpec', baseline: 'a', yours: 'a', spec: 'b' }],
} as OpenApiSyncItem;

/** One new, one plain changed, one conflicting changed, one clean removed and one edited removed. */
function bulkPlan() {
  return makePlan({ items: [newItem, plainChanged, changedItem, cleanRemoved, editedRemoved] });
}

describe('useSyncSelection defaults and bulk actions', () => {
  it('preselects_new_and_changed_items_without_conflicts', () => {
    const { result } = setup(bulkPlan());
    expect([...result.current.approved].sort()).toEqual(['GET /pets', 'PUT /pets']);
    expect(result.current.selectedCount).toBe(2);
  });

  it('a_conflict_item_starts_unselected_and_keeps_yours', () => {
    const { result } = setup(bulkPlan());
    expect(result.current.approved.has('POST /pets')).toBe(false);
    expect(result.current.choices.get('POST /pets')).toEqual({ url: 'yours' });
  });

  it('removed_items_are_never_preselected', () => {
    const { result } = setup(bulkPlan());
    expect(result.current.approved.has('DELETE /pets')).toBe(false);
    expect(result.current.approved.has('DELETE /edited')).toBe(false);
  });

  it('accept_all_selects_new_and_changed_and_takes_the_spec_on_conflicts', () => {
    const { result } = setup(bulkPlan());
    let out = { overwritten: -1 };
    act(() => {
      out = result.current.acceptAll();
    });
    expect(out.overwritten).toBe(1);
    expect([...result.current.approved].sort()).toEqual(['GET /pets', 'POST /pets', 'PUT /pets']);
    expect(result.current.choices.get('POST /pets')).toEqual({ url: 'spec' });
  });

  it('accept_all_never_selects_a_removed_item_the_user_edited', () => {
    const { result } = setup(bulkPlan());
    // Even a manual approval of the edited item is dropped by Accept all.
    act(() => result.current.toggle('DELETE /edited'));
    act(() => {
      result.current.acceptAll();
    });
    expect(result.current.approved.has('DELETE /edited')).toBe(false);
    expect(result.current.selection.items.map((i) => i.key)).not.toContain('DELETE /edited');
  });

  it('keep_current_clears_the_selection', () => {
    const { result } = setup(bulkPlan());
    act(() => result.current.toggleVariable('x'));
    act(() => result.current.keepCurrent());
    expect(result.current.selectedCount).toBe(0);
    expect(result.current.selection.items).toEqual([]);
    expect(result.current.selection.skips).toEqual([]);
  });

  it('skip_all_marks_every_item_skipped_and_selects_nothing', () => {
    const { result } = setup(bulkPlan());
    act(() => result.current.skipAll());
    expect(result.current.selectedCount).toBe(0);
    expect(result.current.selection.items).toEqual([]);
    expect(result.current.selection.skips.map((s) => s.key).sort()).toEqual(
      ['DELETE /edited', 'DELETE /pets', 'GET /pets', 'POST /pets', 'PUT /pets'].sort(),
    );
  });

  it('a_new_plan_resets_an_item_whose_digest_changed_to_its_default', () => {
    const { result, rerender } = setup(bulkPlan());
    act(() => result.current.toggle('GET /pets'));
    expect(result.current.approved.has('GET /pets')).toBe(false);
    rerender({
      p: makePlan({ items: [{ ...newItem, digest: 'changed' }, plainChanged, changedItem] }),
    });
    expect(result.current.approved.has('GET /pets')).toBe(true);
  });

  it('a_new_plan_keeps_the_choice_for_items_whose_digest_did_not_change', () => {
    const { result, rerender } = setup(bulkPlan());
    act(() => result.current.keepCurrent());
    act(() => result.current.toggle('DELETE /pets'));
    act(() => result.current.setChoice('POST /pets', 'url', 'spec'));
    rerender({
      p: makePlan({ items: [{ ...newItem, digest: 'changed' }, plainChanged, changedItem] }),
    });
    // The new item has new content, so it starts from the default. The rest keep the user's choice.
    expect([...result.current.approved]).toEqual(['GET /pets']);
    expect(result.current.choices.get('POST /pets')).toEqual({ url: 'spec' });
  });

  it('an_item_the_user_unticked_stays_unticked_when_another_item_leaves_the_plan', () => {
    const { result, rerender } = setup(bulkPlan());
    act(() => result.current.toggle('PUT /pets'));
    rerender({ p: makePlan({ items: [plainChanged, changedItem] }) });
    expect(result.current.approved.size).toBe(0);
  });

  it('toggling_inside_one_plan_is_kept', () => {
    const { result, rerender } = setup(bulkPlan());
    act(() => result.current.toggle('GET /pets'));
    act(() => result.current.setChoice('POST /pets', 'url', 'spec'));
    rerender({ p: bulkPlan() });
    expect(result.current.approved.has('GET /pets')).toBe(false);
    expect(result.current.choices.get('POST /pets')).toEqual({ url: 'spec' });
  });

  it('keep_current_resets_take_spec_choices_in_the_hook', () => {
    const { result } = setup(bulkPlan());
    act(() => {
      result.current.acceptAll();
    });
    expect(result.current.choices.get('POST /pets')).toEqual({ url: 'spec' });
    act(() => result.current.keepCurrent());
    expect(result.current.choices.get('POST /pets')).toEqual({ url: 'yours' });
  });

  it('accept_all_also_selects_variable_changes', () => {
    const plan = makePlan({
      items: [newItem],
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
    });
    const { result } = setup(plan);
    expect(result.current.variables.size).toBe(1);
    act(() => result.current.keepCurrent());
    expect(result.current.variables.size).toBe(0);
    act(() => {
      result.current.acceptAll();
    });
    expect(result.current.selection.variables).toEqual([{ name: 'baseUrl', digest: 'dv' }]);
  });

  it('skip_all_returns_an_undo_that_restores_the_selection', () => {
    const { result } = setup(bulkPlan());
    let undo = () => {};
    act(() => {
      undo = result.current.skipAll();
    });
    expect(result.current.selectedCount).toBe(0);
    act(() => undo());
    expect([...result.current.approved].sort()).toEqual(['GET /pets', 'PUT /pets']);
    expect(result.current.skips.size).toBe(0);
  });

  it('a_skip_all_undo_is_ignored_after_the_plan_changed', () => {
    const { result, rerender } = setup(bulkPlan());
    let undo = () => {};
    act(() => {
      undo = result.current.skipAll();
    });
    rerender({ p: makePlan({ items: [{ ...newItem, digest: 'other' }] }) });
    act(() => undo());
    expect([...result.current.approved]).toEqual(['GET /pets']);
    expect(result.current.skips.size).toBe(0);
  });

  it('the_defaults_select_variable_changes', () => {
    const plan = makePlan({
      items: [newItem],
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
    });
    const { result } = setup(plan);
    expect(result.current.selectedCount).toBe(2);
  });
});
