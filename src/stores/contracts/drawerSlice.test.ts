import { beforeEach, describe, expect, it } from 'vitest';
import { DEFAULT_FILTERS } from '@/types/contracts';
import { useDrawerStore } from './drawerSlice';

describe('useDrawerStore', () => {
  beforeEach(() => {
    useDrawerStore.setState({
      isOpen: false,
      contractId: null,
      filters: { ...DEFAULT_FILTERS },
    });
  });

  describe('open()', () => {
    it('sets isOpen to true', () => {
      useDrawerStore.getState().open('c1');
      expect(useDrawerStore.getState().isOpen).toBe(true);
    });

    it('sets contractId', () => {
      useDrawerStore.getState().open('c1');
      expect(useDrawerStore.getState().contractId).toBe('c1');
    });

    it('resets filters to defaults', () => {
      useDrawerStore.setState({
        filters: { search: 'foo', kinds: ['add'], breakingOnly: true, sinceSigned: true },
      });
      useDrawerStore.getState().open('c2');
      expect(useDrawerStore.getState().filters).toEqual(DEFAULT_FILTERS);
    });
  });

  describe('close()', () => {
    it('sets isOpen to false', () => {
      useDrawerStore.setState({ isOpen: true, contractId: 'c1' });
      useDrawerStore.getState().close();
      expect(useDrawerStore.getState().isOpen).toBe(false);
    });

    it('preserves contractId so close animation can complete', () => {
      useDrawerStore.setState({ isOpen: true, contractId: 'c1' });
      useDrawerStore.getState().close();
      expect(useDrawerStore.getState().contractId).toBe('c1');
    });
  });

  describe('clearContract()', () => {
    it('sets contractId to null', () => {
      useDrawerStore.setState({ isOpen: false, contractId: 'c1' });
      useDrawerStore.getState().clearContract();
      expect(useDrawerStore.getState().contractId).toBeNull();
    });

    it('does not affect isOpen', () => {
      useDrawerStore.setState({ isOpen: false, contractId: 'c1' });
      useDrawerStore.getState().clearContract();
      expect(useDrawerStore.getState().isOpen).toBe(false);
    });
  });

  describe('setSearch()', () => {
    it('updates the search filter', () => {
      useDrawerStore.getState().setSearch('hello');
      expect(useDrawerStore.getState().filters.search).toBe('hello');
    });

    it('does not touch other filters', () => {
      useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, breakingOnly: true } });
      useDrawerStore.getState().setSearch('world');
      expect(useDrawerStore.getState().filters.breakingOnly).toBe(true);
    });
  });

  describe('toggleKind()', () => {
    it('adds a kind when not present', () => {
      useDrawerStore.getState().toggleKind('add');
      expect(useDrawerStore.getState().filters.kinds).toContain('add');
    });

    it('removes a kind when already present', () => {
      useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, kinds: ['add', 'remove'] } });
      useDrawerStore.getState().toggleKind('add');
      expect(useDrawerStore.getState().filters.kinds).not.toContain('add');
      expect(useDrawerStore.getState().filters.kinds).toContain('remove');
    });
  });

  describe('toggleBreakingOnly()', () => {
    it('toggles breakingOnly from false to true', () => {
      useDrawerStore.getState().toggleBreakingOnly();
      expect(useDrawerStore.getState().filters.breakingOnly).toBe(true);
    });

    it('toggles breakingOnly from true to false', () => {
      useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, breakingOnly: true } });
      useDrawerStore.getState().toggleBreakingOnly();
      expect(useDrawerStore.getState().filters.breakingOnly).toBe(false);
    });
  });

  describe('toggleSinceSigned()', () => {
    it('toggles sinceSigned from false to true', () => {
      useDrawerStore.getState().toggleSinceSigned();
      expect(useDrawerStore.getState().filters.sinceSigned).toBe(true);
    });

    it('toggles sinceSigned from true to false', () => {
      useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, sinceSigned: true } });
      useDrawerStore.getState().toggleSinceSigned();
      expect(useDrawerStore.getState().filters.sinceSigned).toBe(false);
    });
  });

  describe('resetFilters()', () => {
    it('resets all filters to DEFAULT_FILTERS', () => {
      useDrawerStore.setState({
        filters: { search: 'foo', kinds: ['add', 'modify'], breakingOnly: true, sinceSigned: true },
      });
      useDrawerStore.getState().resetFilters();
      expect(useDrawerStore.getState().filters).toEqual(DEFAULT_FILTERS);
    });

    it('does not affect isOpen or contractId', () => {
      useDrawerStore.setState({
        isOpen: true,
        contractId: 'c1',
        filters: { ...DEFAULT_FILTERS, search: 'x' },
      });
      useDrawerStore.getState().resetFilters();
      expect(useDrawerStore.getState().isOpen).toBe(true);
      expect(useDrawerStore.getState().contractId).toBe('c1');
    });
  });
});
