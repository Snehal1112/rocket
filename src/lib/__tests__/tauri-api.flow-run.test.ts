import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { runFlow } from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('runFlow', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue({ runId: 'r1', steps: [], stoppedReason: 'completed' });
  });

  it('sends a full run without a partial key', async () => {
    await runFlow('c', 'f', null, null);
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: { collection: 'c', flowName: 'f', environmentName: null, globalEnvName: null },
    });
  });

  it('sends a partial run request next to the run id', async () => {
    await runFlow('c', 'f', null, null, undefined, {
      runId: 'run-1',
      partial: { baseRunId: '01A', startNodeId: 'n2', mode: 'fromHere' },
    });
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: {
        collection: 'c',
        flowName: 'f',
        environmentName: null,
        globalEnvName: null,
        runId: 'run-1',
        partial: { baseRunId: '01A', startNodeId: 'n2', mode: 'fromHere' },
      },
    });
  });
});
