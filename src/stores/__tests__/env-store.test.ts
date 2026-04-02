import { vi, describe, it, expect, beforeEach } from 'vitest';
import { useEnvStore } from '../env-store';
import type { Environment } from '@/lib/tauri-api';

// Mock tauri-api so tests don't touch the Tauri IPC layer.
vi.mock('@/lib/tauri-api', () => ({
  listEnvironments: vi.fn(),
  saveEnvironment: vi.fn(),
  deleteEnvironment: vi.fn(),
  getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
  setGlobalEnvironment: vi.fn().mockResolvedValue(undefined),
  getProcessEnvVars: vi.fn().mockResolvedValue({}),
  getEnvironment: vi.fn().mockResolvedValue(null),
  getGlobalEnvironment: vi.fn().mockResolvedValue(null),
  listGlobalEnvironments: vi.fn().mockResolvedValue([]),
  saveGlobalEnvironment: vi.fn().mockResolvedValue(undefined),
  deleteGlobalEnvironment: vi.fn().mockResolvedValue(undefined),
}));

import { listEnvironments, getGlobalEnvironmentName } from '@/lib/tauri-api';

const mockListEnvironments = vi.mocked(listEnvironments);
const mockGetGlobalEnvironmentName = vi.mocked(getGlobalEnvironmentName);

const makeEnv = (name: string): Environment => ({ name, variables: [] });

describe('env-store', () => {
  beforeEach(() => {
    localStorage.clear();
    // Reset store to initial state before each test.
    useEnvStore.setState({
      environments: [],
      activeEnvId: null,
      activeCollection: null,
    });
    vi.clearAllMocks();
  });

  it('loadEnvironments sets activeCollection', async () => {
    mockListEnvironments.mockResolvedValueOnce([]);

    await useEnvStore.getState().loadEnvironments('my-collection');

    expect(useEnvStore.getState().activeCollection).toBe('my-collection');
  });

  it('setActiveEnv scopes localStorage key to collection', () => {
    useEnvStore.setState({ activeCollection: 'col-a', activeEnvId: null });

    useEnvStore.getState().setActiveEnv('dev');

    expect(localStorage.getItem('rocket-api:active-env:col-a')).toBe('dev');
    // Unrelated key must not exist.
    expect(localStorage.getItem('rocket-api:active-env:col-b')).toBeNull();

    useEnvStore.getState().setActiveEnv(null);
    expect(localStorage.getItem('rocket-api:active-env:col-a')).toBeNull();
  });

  it('loadEnvironments restores activeEnvId from per-collection localStorage', async () => {
    const envs = [makeEnv('staging'), makeEnv('production')];
    mockListEnvironments.mockResolvedValueOnce(envs);

    // Simulate a previously persisted selection for this collection.
    localStorage.setItem('rocket-api:active-env:my-collection', 'staging');

    await useEnvStore.getState().loadEnvironments('my-collection');

    expect(useEnvStore.getState().activeEnvId).toBe('staging');
  });

  it('loadEnvironments clears environments and activeEnvId on error', async () => {
    // Seed some prior state.
    useEnvStore.setState({ environments: [makeEnv('old')], activeEnvId: 'old', activeCollection: 'col-a' });
    mockListEnvironments.mockRejectedValueOnce(new Error('network'));

    await useEnvStore.getState().loadEnvironments('col-a');

    expect(useEnvStore.getState().environments).toEqual([]);
    expect(useEnvStore.getState().activeEnvId).toBeNull();
  });

  it('loadEnvironments discards result if collection changed mid-flight', async () => {
    const { listEnvironments: mockApi } = await import('@/lib/tauri-api');
    const mock = vi.mocked(mockApi);

    // First call resolves after a microtask delay.
    let resolveFirst!: (v: Environment[]) => void;
    mock.mockImplementationOnce(() => new Promise(r => { resolveFirst = r; }));
    // Second call resolves immediately.
    mock.mockResolvedValueOnce([makeEnv('staging')]);

    const p1 = useEnvStore.getState().loadEnvironments('col-a');
    const p2 = useEnvStore.getState().loadEnvironments('col-b');

    // Resolve col-a after col-b has already taken over.
    resolveFirst([makeEnv('dev')]);
    await Promise.all([p1, p2]);

    // col-b should win.
    expect(useEnvStore.getState().activeCollection).toBe('col-b');
    expect(useEnvStore.getState().environments).toEqual([makeEnv('staging')]);
  });

  it('createEnvironment saves env and sets it active', async () => {
    const { saveEnvironment: mockSave } = await import('@/lib/tauri-api');
    vi.mocked(mockSave).mockResolvedValueOnce(undefined);
    mockListEnvironments.mockResolvedValueOnce([makeEnv('dev')]);

    useEnvStore.setState({ activeCollection: 'col-a' });
    await useEnvStore.getState().createEnvironment('dev');

    expect(useEnvStore.getState().activeEnvId).toBe('dev');
  });

  it('deleteEnvironment removes env from list', async () => {
    const { deleteEnvironment: mockDelete } = await import('@/lib/tauri-api');
    vi.mocked(mockDelete).mockResolvedValueOnce(undefined);

    useEnvStore.setState({
      activeCollection: 'col-a',
      environments: [makeEnv('dev'), makeEnv('staging')],
      activeEnvId: 'staging',
    });

    await useEnvStore.getState().deleteEnvironment('dev');

    expect(useEnvStore.getState().environments).toEqual([makeEnv('staging')]);
    expect(useEnvStore.getState().activeEnvId).toBe('staging'); // unchanged
  });

  it('updateEnvironment replaces env in list', async () => {
    const { saveEnvironment: mockSave } = await import('@/lib/tauri-api');
    vi.mocked(mockSave).mockResolvedValueOnce(undefined);

    const updated: Environment = { name: 'dev', variables: [{ key: 'X', value: '1', enabled: true, secret: false }] };
    useEnvStore.setState({
      activeCollection: 'col-a',
      environments: [makeEnv('dev'), makeEnv('staging')],
    });

    await useEnvStore.getState().updateEnvironment(updated);

    expect(useEnvStore.getState().environments[0]).toEqual(updated);
  });
});

describe('getGlobalVariables', () => {
  beforeEach(() => {
    useEnvStore.setState({ globalEnvName: null, globalEnv: null, processEnvVars: {} });
    vi.clearAllMocks();
  });

  it('returns enabled vars only', () => {
    useEnvStore.setState({
      globalEnv: {
        name: 'shared',
        variables: [
          { key: 'A', value: 'a', enabled: true },
          { key: 'B', value: 'b', enabled: false },
        ],
      } as any,
    });
    const vars = useEnvStore.getState().getGlobalVariables();
    expect(vars['A']).toBe('a');
    expect(vars['B']).toBeUndefined();
  });

  it('returns empty object when globalEnv is null', () => {
    useEnvStore.setState({ globalEnv: null });
    expect(useEnvStore.getState().getGlobalVariables()).toEqual({});
  });
});

describe('fetchGlobalEnv', () => {
  beforeEach(() => {
    useEnvStore.setState({ globalEnvName: null, globalEnv: null, processEnvVars: {} });
    vi.clearAllMocks();
  });

  it('null name clears state', async () => {
    mockGetGlobalEnvironmentName.mockResolvedValueOnce(null);
    await useEnvStore.getState().fetchGlobalEnv();
    expect(useEnvStore.getState().globalEnv).toBeNull();
    expect(useEnvStore.getState().globalEnvName).toBeNull();
  });

  it('sets globalEnv when the workspace-level env file exists', async () => {
    const env: Environment = { name: 'shared', variables: [] };
    mockGetGlobalEnvironmentName.mockResolvedValueOnce('shared');
    const { getGlobalEnvironment: mockGetGlobalEnv } = await import('@/lib/tauri-api');
    vi.mocked(mockGetGlobalEnv).mockResolvedValueOnce(env);

    await useEnvStore.getState().fetchGlobalEnv();

    expect(useEnvStore.getState().globalEnvName).toBe('shared');
    expect(useEnvStore.getState().globalEnv).toEqual(env);
  });
});
