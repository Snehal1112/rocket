import { describe, expect, it } from 'vitest';
import { nodeStatusCaption, nodeStatusClassName } from '../nodeStatus';

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
