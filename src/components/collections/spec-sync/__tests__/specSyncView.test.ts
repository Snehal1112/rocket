import { describe, expect, it } from 'vitest';
import {
  defaultChoice,
  fieldLabel,
  filterItems,
  filterVariables,
  firstEntryId,
  groupItems,
  isPendingChangedError,
  outcomeLabel,
  renameHint,
} from '../specSyncView';
import { changedItem, makePlan, newItem, removedItem } from './fixtures';

describe('specSyncView', () => {
  it('groups New, Changed, Removed in that order with counts', () => {
    const groups = groupItems([removedItem, changedItem, newItem, { ...newItem, key: 'x' }]);
    expect(groups.map((g) => [g.label, g.items.length])).toEqual([
      ['New', 2],
      ['Changed', 1],
      ['Removed', 1],
    ]);
  });

  it('labels the three outcomes', () => {
    expect(outcomeLabel('takeSpec')).toBe('Spec wins');
    expect(outcomeLabel('keepYours')).toBe('Yours kept');
    expect(outcomeLabel('conflict')).toBe('Conflict');
  });

  it('builds rename hints for new and removed items', () => {
    expect(renameHint(newItem)).toBe('Possibly renamed from GET /animals');
    expect(renameHint(removedItem)).toBe('Possibly renamed to DELETE /animals');
    expect(renameHint(changedItem)).toBeNull();
  });

  it('maps field names to display names', () => {
    expect(fieldLabel('header:Accept')).toBe('Header Accept');
    expect(fieldLabel('body:$.a.b')).toBe('Body $.a.b');
  });

  it('selects the first entry of the first non-empty group', () => {
    expect(firstEntryId(makePlan())).toBe('new:GET /pets');
    expect(firstEntryId(makePlan({ items: [] }))).toBeNull();
  });

  it('detects the pending-changed conflict', () => {
    expect(isPendingChangedError('Conflict: the pending spec changed, check again')).toBe(true);
    expect(isPendingChangedError(new Error('boom'))).toBe(false);
    expect(isPendingChangedError('Conflict: an earlier apply is not finished yet')).toBe(false);
  });

  it('defaults a choice from the outcome, conflicts go to yours', () => {
    expect(defaultChoice('takeSpec')).toBe('spec');
    expect(defaultChoice('keepYours')).toBe('yours');
    expect(defaultChoice('conflict')).toBe('yours');
  });

  it('filters items by name or key, ignoring case, and keeps all for an empty query', () => {
    const items = [newItem, changedItem, removedItem];
    expect(filterItems(items, '')).toHaveLength(3);
    expect(filterItems(items, '  ')).toHaveLength(3);
    expect(filterItems(items, 'add PET').map((i) => i.key)).toEqual(['POST /pets']);
    expect(filterItems(items, 'delete /pets').map((i) => i.key)).toEqual(['DELETE /pets']);
    expect(filterItems(items, 'zzz')).toEqual([]);
  });

  it('filters variable changes by label', () => {
    const vars = [
      { name: 'baseUrl', kind: 'baseUrl' as const, digest: 'a' },
      { name: 'apiKey', kind: 'addSecurity' as const, digest: 'b' },
    ];
    expect(filterVariables(vars, 'security').map((v) => v.name)).toEqual(['apiKey']);
    expect(filterVariables(vars, '')).toHaveLength(2);
  });
});
