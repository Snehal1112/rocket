import { describe, expect, it } from 'vitest';
import {
  issueRingClassName,
  nodeStatusCaption,
  nodeStatusClassName,
  nodeStatusLabel,
} from '../nodeStatus';

describe('nodeStatusClassName', () => {
  it('keeps the existing per-status styles', () => {
    expect(nodeStatusClassName('idle')).toBe('border-border');
    expect(nodeStatusClassName('success')).toContain('border-green-500');
    expect(nodeStatusClassName('failed')).toContain('border-red-500');
    expect(nodeStatusClassName('running')).toContain('animate-pulse');
  });

  it('fades an upstream-failed skip without a dashed border', () => {
    const cls = nodeStatusClassName('skipped', 'upstream_failed');
    expect(cls).toContain('opacity-60');
    expect(cls).not.toContain('border-dashed');
  });

  it('treats a skip with no reason like an upstream-failed skip', () => {
    expect(nodeStatusClassName('skipped')).toBe(nodeStatusClassName('skipped', 'upstream_failed'));
  });

  it('fades and dashes a not-taken skip', () => {
    const cls = nodeStatusClassName('skipped', 'branch_not_taken');
    expect(cls).toContain('border-dashed');
    expect(cls).toContain('opacity-50');
  });
});

describe('nodeStatusCaption', () => {
  it('has no caption unless the node was skipped', () => {
    expect(nodeStatusCaption('idle')).toBeNull();
    expect(nodeStatusCaption('success', { skipReason: 'branch_not_taken' })).toBeNull();
  });

  it('names each skip reason', () => {
    expect(nodeStatusCaption('skipped', { skipReason: 'upstream_failed' })).toBe(
      'Skipped — upstream failed',
    );
    expect(nodeStatusCaption('skipped', { skipReason: 'branch_not_taken' })).toBe('Not taken');
    expect(nodeStatusCaption('skipped')).toBe('Skipped — upstream failed');
  });
});

describe('issueRingClassName', () => {
  const error = { code: 'a', severity: 'error' as const, nodeId: 'n', message: 'm' };
  const warning = { code: 'b', severity: 'warning' as const, nodeId: 'n', message: 'm' };

  it('draws a red ring for an error, even next to a warning', () => {
    expect(issueRingClassName([error])).toContain('ring-red-500');
    expect(issueRingClassName([warning, error])).toContain('ring-red-500');
  });

  it('draws an amber ring, and no red, for a warning', () => {
    const cls = issueRingClassName([warning]);
    expect(cls).toContain('ring-amber-500');
    expect(cls).not.toContain('ring-red-500');
  });

  it('draws no ring without issues', () => {
    expect(issueRingClassName(undefined)).toBeUndefined();
    expect(issueRingClassName([])).toBeUndefined();
  });
});

describe('nodeStatusLabel', () => {
  it('names every status in words', () => {
    expect(nodeStatusLabel('idle')).toBe('not run');
    expect(nodeStatusLabel('running')).toBe('running');
    expect(nodeStatusLabel('success')).toBe('succeeded');
    expect(nodeStatusLabel('failed')).toBe('failed');
  });

  it('says why a node was skipped', () => {
    expect(nodeStatusLabel('skipped', { skipReason: 'branch_not_taken' })).toBe(
      'skipped, branch not taken',
    );
    expect(nodeStatusLabel('skipped', { skipReason: 'upstream_failed' })).toBe(
      'skipped, upstream failed',
    );
  });

  it('treats a skip with no reason as an upstream failure, like the caption does', () => {
    expect(nodeStatusLabel('skipped')).toBe('skipped, upstream failed');
  });
});
