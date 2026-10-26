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
    act(() => result.current.toggle('GET /pets'));
    expect(result.current.approved.has('GET /pets')).toBe(true);
    expect(result.current.selection.items.map((i) => i.key)).toEqual(['GET /pets']);
    act(() => result.current.toggle('GET /pets'));
    expect(result.current.approved.size).toBe(0);
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
    expect(result.current.selection.items).toEqual([]);
    act(() => result.current.toggle('POST /pets'));
    expect(result.current.selection.items[0]?.choices).toEqual({ url: 'spec' });
  });

  it('approves variables on their own', () => {
    const plan = makePlan({
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dv' }],
    });
    const { result } = setup(plan);
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
    expect(result.current.approved.size).toBe(0);
  });

  it('reset clears the selection but keeps the links', () => {
    const { result } = setup();
    act(() => result.current.toggle('POST /pets'));
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
