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

export const useEnvStore = create<EnvState>((set, get) => ({
  environments: [],
  activeEnvId: null,

  async loadEnvironments() {
    try {
      const environments = await listEnvironments();
      set({ environments });
    } catch (err) {
      console.error('[EnvStore] Failed to load environments:', err);
    }
  },

  setActiveEnv(id) {
    set({ activeEnvId: id });
  },

  async createEnvironment(name) {
    const env: Environment = { name, variables: [] };
    await saveEnvironment(env);
    await get().loadEnvironments();
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
