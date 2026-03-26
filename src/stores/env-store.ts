import { create } from 'zustand';
import {
  listEnvironments,
  saveEnvironment,
  deleteEnvironment as deleteEnvApi,
  type Environment,
} from '@/lib/tauri-api';

// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export interface EnvState {
  environments: Environment[];
  activeEnvId: string | null;

  loadEnvironments: () => Promise<void>;
  setActiveEnv: (id: string | null) => void;
  createEnvironment: (name: string) => Promise<void>;
  updateEnvironment: (env: Environment) => Promise<void>;
  deleteEnvironment: (name: string) => Promise<void>;
  getActiveVariables: () => Record<string, string>;
  resolveVariables: (text: string) => string;
}

const STORAGE_KEY = 'rocket-active-env';

export const useEnvStore = create<EnvState>((set, get) => ({
  environments: [],
  activeEnvId: localStorage.getItem(STORAGE_KEY),

  async loadEnvironments() {
    try {
      const environments = await listEnvironments();
      set({ environments });
    } catch (err) {
      console.error('[EnvStore] Failed to load environments:', err);
    }
  },

  setActiveEnv(id) {
    if (id) {
      localStorage.setItem(STORAGE_KEY, id);
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
    set({ activeEnvId: id });
  },

  async createEnvironment(name) {
    const env: Environment = { name, variables: [] };
    await saveEnvironment(env);
    await get().loadEnvironments();
    localStorage.setItem(STORAGE_KEY, name);
    set({ activeEnvId: name });
  },

  async updateEnvironment(env) {
    await saveEnvironment(env);
    set((state) => ({
      environments: state.environments.map((e) =>
        e.name === env.name ? env : e,
      ),
    }));
  },

  async deleteEnvironment(name) {
    await deleteEnvApi(name);
    if (get().activeEnvId === name) localStorage.removeItem(STORAGE_KEY);
    set((state) => ({
      environments: state.environments.filter((e) => e.name !== name),
      activeEnvId: state.activeEnvId === name ? null : state.activeEnvId,
    }));
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
}));
