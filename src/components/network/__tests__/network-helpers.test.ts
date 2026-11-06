import { describe, expect, it } from 'vitest';
import {
  filterNodes,
  groupNodes,
  sortNodes,
  validateAddress,
  validateDisplayName,
} from '@/components/network/network-helpers';
import { makeNode } from '@/test/network-fixtures';

const ana = makeNode({
  deviceId: '1',
  displayName: 'Ana Lima',
  initials: 'AL',
  shortId: 'K7QF-2M9X',
});
const ben = makeNode({
  deviceId: '2',
  displayName: 'Ben Ortiz',
  initials: 'BO',
  shortId: 'AAAA-1111',
});
const ids = (list: { deviceId: string }[]) => list.map((n) => n.deviceId);

describe('filterNodes', () => {
  it('returns the same list for an empty or blank query', () => {
    const nodes = [ana, ben];
    expect(filterNodes(nodes, '')).toBe(nodes);
    expect(filterNodes(nodes, '   ')).toBe(nodes);
  });

  it('matches the name without caring about case', () => {
    expect(ids(filterNodes([ana, ben], 'ANA li'))).toEqual(['1']);
  });

  it('matches the initials', () => {
    expect(ids(filterNodes([ana, ben], 'bo'))).toEqual(['2']);
  });

  it('matches the short id with or without the dash', () => {
    expect(ids(filterNodes([ana, ben], 'k7qf-2m'))).toEqual(['1']);
    expect(ids(filterNodes([ana, ben], 'k7qf2m'))).toEqual(['1']);
  });

  it('returns an empty list when nothing matches', () => {
    expect(filterNodes([ana, ben], 'zzz')).toEqual([]);
  });

  it('finds one of two nodes that share a name by its short id', () => {
    const twin = makeNode({ deviceId: '3', displayName: 'Ben Ortiz', shortId: 'BBBB-2222' });
    expect(ids(filterNodes([ben, twin], 'bbbb2222'))).toEqual(['3']);
  });
});

describe('sortNodes', () => {
  it('puts online first and offline last, then sorts by name', () => {
    const list = [
      makeNode({ deviceId: 'off', displayName: 'A', status: 'offline' }),
      makeNode({ deviceId: 'on-z', displayName: 'Z', status: 'online' }),
      makeNode({ deviceId: 'on-b', displayName: 'B', status: 'online' }),
      makeNode({ deviceId: 'pair', displayName: 'C', status: 'pairing' }),
      makeNode({ deviceId: 'upd', displayName: 'D', status: 'needs_update' }),
      makeNode({ deviceId: 'av', displayName: 'E', status: 'available' }),
    ];
    expect(ids(sortNodes(list))).toEqual(['on-b', 'on-z', 'pair', 'upd', 'av', 'off']);
  });

  it('does not change the input', () => {
    const list = [
      makeNode({ deviceId: 'off', status: 'offline' }),
      makeNode({ deviceId: 'on', status: 'online' }),
    ];
    sortNodes(list);
    expect(ids(list)).toEqual(['off', 'on']);
  });
});

describe('groupNodes', () => {
  it('splits paired from nearby', () => {
    const groups = groupNodes([
      makeNode({ deviceId: 'a', group: 'paired' }),
      makeNode({ deviceId: 'b', group: 'nearby' }),
    ]);
    expect(ids(groups.paired)).toEqual(['a']);
    expect(ids(groups.nearby)).toEqual(['b']);
  });
});

describe('validateAddress', () => {
  it('trims the host and parses the port', () => {
    expect(validateAddress('  10.0.0.5 ', '47821')).toEqual({
      ok: true,
      host: '10.0.0.5',
      port: 47821,
    });
    expect(validateAddress('mac.tail1234.ts.net', '1')).toMatchObject({ ok: true, port: 1 });
    expect(validateAddress('h', '65535')).toMatchObject({ ok: true, port: 65535 });
  });

  it('asks for a host', () => {
    expect(validateAddress('', '47821')).toEqual({
      ok: false,
      error: 'Enter a host name or IP address.',
    });
    expect(validateAddress('   ', '47821')).toMatchObject({ ok: false });
  });

  it('refuses spaces and paths in the host', () => {
    const error = 'Enter only the host, without spaces or a path.';
    expect(validateAddress('a b', '47821')).toEqual({ ok: false, error });
    expect(validateAddress('http://x', '47821')).toEqual({ ok: false, error });
    expect(validateAddress('x/y', '47821')).toEqual({ ok: false, error });
  });

  it.each(['0', '65536', '80a', '', '-1', '1.5', '123456'])('refuses the port "%s"', (port) => {
    expect(validateAddress('host', port)).toEqual({
      ok: false,
      error: 'Port must be a number from 1 to 65535.',
    });
  });
});

describe('validateDisplayName', () => {
  it('trims the name', () => {
    expect(validateDisplayName('  Ana Lima ')).toEqual({ ok: true, name: 'Ana Lima' });
  });

  it('refuses an empty or blank name', () => {
    expect(validateDisplayName('')).toEqual({ ok: false, error: 'Enter a name.' });
    expect(validateDisplayName('   ')).toEqual({ ok: false, error: 'Enter a name.' });
  });

  it('accepts 40 characters and refuses 41', () => {
    expect(validateDisplayName('a'.repeat(40))).toMatchObject({ ok: true });
    expect(validateDisplayName('a'.repeat(41))).toEqual({
      ok: false,
      error: 'Use 40 characters or fewer.',
    });
  });

  it('counts an emoji as one character, like the backend', () => {
    expect(validateDisplayName('😀'.repeat(40))).toMatchObject({ ok: true });
    expect(validateDisplayName('😀'.repeat(41))).toMatchObject({ ok: false });
  });

  it('refuses control characters', () => {
    expect(validateDisplayName('Ana\tLima')).toEqual({
      ok: false,
      error: 'The name cannot contain control characters.',
    });
  });
});
