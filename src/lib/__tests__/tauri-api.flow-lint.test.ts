import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type Flow, lintFlow } from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const flow: Flow = { name: 'f', nodes: [], edges: [] };

describe('lintFlow', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('sends the unsaved graph and returns the lints', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await expect(lintFlow('demo', flow)).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledWith('lint_flow', { collection: 'demo', flow });
  });
});
