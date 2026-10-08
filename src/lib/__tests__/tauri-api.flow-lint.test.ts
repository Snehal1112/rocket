import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type Flow, type FlowLint, lintFlow } from '../tauri-api';
import fixture from './fixtures/flow-lint.json';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const flow: Flow = { name: 'f', nodes: [], edges: [] };
// Typing the fixture makes tsc check its keys against the FlowLint type.
const lints: FlowLint[] = fixture as FlowLint[];

describe('lintFlow', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('sends the unsaved graph and returns the lints', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await expect(lintFlow('demo', flow)).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledWith('lint_flow', { collection: 'demo', flow });
  });

  it('returns the golden fixture shape shared with the Rust DTO', async () => {
    vi.mocked(invoke).mockResolvedValue(lints);
    const result = await lintFlow('demo', flow);
    expect(result).toEqual(lints);
    expect(result[0].nodeId).toBe('check');
    expect(result[0].hint).toBe('Wire it to a node, or remove the branch.');
    expect(result[0].severity).toBe('warning');
    expect(result[1].edgeId).toBe('e7');
    expect(result[1].nodeId).toBeUndefined();
    expect(result[1].severity).toBe('error');
  });
});
