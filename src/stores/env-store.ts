import { create } from 'zustand';
import {
  listEnvironments,
  saveEnvironment,
  deleteEnvironment as deleteEnvApi,
  getGlobalEnvironmentName,
  setGlobalEnvironment,
  getProcessEnvVars,
  getEnvironment,
  type Environment,
} from '@/lib/tauri-api';

// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export interface EnvState {
  environments: Environment[];
  activeEnvId: string | null;
  activeCollection: string | null;

  loadEnvironments: (collection: string) => Promise<void>;
  setActiveEnv: (id: string | null) => void;
  createEnvironment: (name: string) => Promise<void>;
  updateEnvironment: (env: Environment) => Promise<void>;
  deleteEnvironment: (name: string) => Promise<void>;
  getActiveVariables: () => Record<string, string>;
  resolveVariables: (text: string) => string;

  // Global environment state.
  globalEnvName: string | null;
  globalEnv: Environment | null;
  processEnvVars: Record<string, string>;

  // Global environment actions.
  fetchGlobalEnv: () => Promise<void>;
  setGlobalEnv: (name: string | null) => Promise<void>;
  loadProcessEnvVars: () => Promise<void>;
  getGlobalVariables: () => Record<string, string>;
}

export const useEnvStore = create<EnvState>((set, get) => ({
  environments: [],
  activeEnvId: null,
  activeCollection: null,
  globalEnvName: null,
  globalEnv: null,
  processEnvVars: {},

  async loadEnvironments(collection) {
    set({ activeCollection: collection });
    try {
      const environments = await listEnvironments(collection);
      // Discard if another collection was loaded while we awaited.
      if (get().activeCollection !== collection) return;
      set({ environments });
      const stored = localStorage.getItem(`rocket-api:active-env:${collection}`);
      const found = environments.find((e) => e.name === stored);
      set({ activeEnvId: found?.name ?? null });
    } catch (err) {
      console.error('[EnvStore] Failed to load environments:', err);
      // Only reset state if this collection is still active.
      if (get().activeCollection === collection) {
        set({ environments: [], activeEnvId: null });
      }
    }
  },

  setActiveEnv(id) {
    const { activeCollection } = get();
    set({ activeEnvId: id });
    const key = activeCollection ? `rocket-api:active-env:${activeCollection}` : null;
    if (key) {
      if (id) localStorage.setItem(key, id);
      else localStorage.removeItem(key);
    }
  },

  async createEnvironment(name) {
    const { activeCollection } = get();
    if (!activeCollection) throw new Error('No active collection');
    const env: Environment = { name, variables: [] };
    await saveEnvironment(activeCollection, env);
    await get().loadEnvironments(activeCollection);
    get().setActiveEnv(name);
  },

  async updateEnvironment(env) {
    const { activeCollection } = get();
    if (!activeCollection) throw new Error('No active collection');
    await saveEnvironment(activeCollection, env);
    set((state) => ({
      environments: state.environments.map((e) => (e.name === env.name ? env : e)),
    }));
  },

  async deleteEnvironment(name) {
    const { activeCollection } = get();
    if (!activeCollection) throw new Error('No active collection');
    await deleteEnvApi(activeCollection, name);
    set((state) => ({
      environments: state.environments.filter((e) => e.name !== name),
      activeEnvId: state.activeEnvId === name ? null : state.activeEnvId,
    }));
    if (get().activeEnvId === null && activeCollection) {
      localStorage.removeItem(`rocket-api:active-env:${activeCollection}`);
    }
  },

  getActiveVariables() {
    const { environments, activeEnvId } = get();
    if (!activeEnvId) return {};
    const env = environments.find((e) => e.name === activeEnvId);
    if (!env) return {};
    const vars: Record<string, string> = {};
    for (const v of env.variables) {
      if (v.enabled) vars[v.key] = v.value;
    }
    return vars;
  },

  resolveVariables(text) {
    const vars = get().getActiveVariables();
    return text.replace(VAR_REGEX, (match, key) => {
      return key in vars ? vars[key] : match;
    });
  },

  async fetchGlobalEnv() {
    const name = await getGlobalEnvironmentName();
    if (!name) { set({ globalEnvName: null, globalEnv: null }); return; }
    const { activeCollection } = get();
    if (activeCollection) {
      try {
        const env = await getEnvironment(activeCollection, name);
        set({ globalEnvName: name, globalEnv: env }); return;
      } catch {}
    }
    set({ globalEnvName: name, globalEnv: null });
  },

  async setGlobalEnv(name) {
    await setGlobalEnvironment(name);
    await get().fetchGlobalEnv();
  },

  async loadProcessEnvVars() {
    set({ processEnvVars: await getProcessEnvVars() });
  },

  getGlobalVariables() {
    const { globalEnv } = get();
    if (!globalEnv) return {};
    return Object.fromEntries(
      globalEnv.variables.filter(v => v.enabled).map(v => [v.key, v.value])
    );
  },
}));
