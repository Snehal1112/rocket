import { describe, expect, it } from 'vitest';
import {
  findAffectedTabs,
  hasDirtyScriptTabs,
  isPathWithin,
} from '@/components/collections/tree-utils';
import type { FlowTab, LeafNode, ScriptTab } from '@/types/pane-types';

function scriptTab(path: string, isDirty = false): ScriptTab {
  return {
    id: `s:${path}`,
    title: path,
    tabType: 'script',
    collectionName: 'col',
    scriptPath: path,
    content: '',
    savedContent: '',
    isDirty,
    source: { collection: 'col', path },
  };
}

function leaf(tabs: ScriptTab[]): LeafNode {
  return { type: 'leaf', groupId: 'g1', tabs, activeTabId: tabs[0]?.id ?? null } as LeafNode;
}

const folder = { type: 'folder' as const, collection: 'col', path: 'lib', name: 'lib' };

describe('tree-utils delete matching', () => {
  it('matches whole path segments only', () => {
    expect(isPathWithin('lib/a.js', 'lib')).toBe(true);
    expect(isPathWithin('lib', 'lib')).toBe(true);
    expect(isPathWithin('lib2/a.js', 'lib')).toBe(false);
  });

  it('does not treat a sibling prefix folder as affected', () => {
    const root = leaf([scriptTab('lib/a.js'), scriptTab('lib2/b.js')]);
    const hit = findAffectedTabs(root, folder);
    expect(hit.map((h) => h.tab.id)).toEqual(['s:lib/a.js']);
  });

  it('reports dirty script tabs under a folder', () => {
    const root = leaf([scriptTab('lib/a.js', true), scriptTab('lib2/b.js')]);
    expect(hasDirtyScriptTabs(root, folder)).toBe(true);
    expect(hasDirtyScriptTabs(root, { ...folder, path: 'lib2' })).toBe(false);
  });
});

const flowTab = (id: string, flowName: string, collectionName = 'col'): FlowTab => ({
  id,
  title: `Flow: ${flowName}`,
  isDirty: false,
  tabType: 'flow',
  collectionName,
  flowName,
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

describe('tree-utils flow delete matching', () => {
  const target = { type: 'flow' as const, collection: 'col', name: 'Login' };

  it('matches the open tab of exactly that flow', () => {
    const root = {
      type: 'leaf',
      groupId: 'g1',
      tabs: [
        flowTab('f1', 'Login'),
        flowTab('f2', 'login'),
        flowTab('f3', 'Login', 'other'),
        scriptTab('lib/a.js'),
      ],
      activeTabId: 'f1',
    } as LeafNode;
    expect(findAffectedTabs(root, target).map((h) => h.tab.id)).toEqual(['f1']);
  });

  it('does not treat a flow target as a script or request target', () => {
    const root = {
      type: 'leaf',
      groupId: 'g1',
      tabs: [scriptTab('Login')],
      activeTabId: 's:Login',
    } as LeafNode;
    expect(findAffectedTabs(root, target)).toEqual([]);
  });
});
