import type { Node } from '@xyflow/react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { minimapNodeColor } from '../minimap';

const node = (type: string, status?: string): Node => ({
  id: 'n',
  type,
  position: { x: 0, y: 0 },
  data: status ? { status } : {},
});

describe('minimapNodeColor', () => {
  it('colours a node by run status first', () => {
    expect(minimapNodeColor(node('Request', 'success'))).toBe('#22c55e');
    expect(minimapNodeColor(node('Request', 'failed'))).toBe('#ef4444');
    expect(minimapNodeColor(node('Request', 'running'))).toBe('#3b82f6');
    expect(minimapNodeColor(node('Request', 'skipped'))).toBe('#9ca3af');
  });

  it('falls back to the node kind when idle or unknown', () => {
    const request = minimapNodeColor(node('Request', 'idle'));
    expect(request).toBe(minimapNodeColor(node('Request')));
    expect(request).not.toBe(minimapNodeColor(node('Output')));
  });

  it('gives every kind a plain hex colour', () => {
    for (const kind of [
      'Request',
      'Input',
      'Output',
      'If',
      'Switch',
      'Transform',
      'WaitForCallback',
      'Auth',
      'SomethingNew',
    ]) {
      expect(minimapNodeColor(node(kind))).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});

describe('minimap styling', () => {
  // WebKitGTK hangs while painting these on this machine, so they must never appear.
  it.each(['minimap.ts', 'FlowCanvas.tsx'])('%s avoids backdrop-filter and color-mix', (file) => {
    const source = readFileSync(resolve(__dirname, '..', file), 'utf8');
    expect(source).not.toMatch(/backdrop-filter|backdrop-blur|backdrop-|color-mix/);
  });
});
