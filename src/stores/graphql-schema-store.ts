import type { GraphQLSchema } from 'graphql';
import { create } from 'zustand';
import { buildSchemaFromIntrospection } from '@/lib/graphql-schema';
import { buildSchemaRequestInput } from '@/lib/graphql-schema-input';
import {
  fetchGraphQlSchema,
  type GraphQlSchemaResult,
  getCachedGraphQlSchema,
} from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

export interface SchemaEntry {
  status: 'idle' | 'loading' | 'ready' | 'error';
  schema?: GraphQLSchema;
  fetchedAt?: string;
  error?: string;
}

interface GraphQlSchemaState {
  /** One entry per request tab. */
  entries: Record<string, SchemaEntry>;
  /** Reads the backend cache. Never touches the network. */
  loadCached: (tabId: string, request: RequestState) => Promise<void>;
  /** Fetches by introspection, or uses the backend cache unless `refresh` is set. */
  fetchSchema: (tabId: string, request: RequestState, refresh: boolean) => Promise<void>;
  clear: (tabId: string) => void;
}

// The newest operation per tab. A reply from an older one is dropped, so a slow fetch
// for a previous URL or environment cannot overwrite the current state.
const latest: Record<string, number> = {};
let counter = 0;

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function ready(result: GraphQlSchemaResult): SchemaEntry {
  return {
    status: 'ready',
    schema: buildSchemaFromIntrospection(result.introspection),
    fetchedAt: result.fetchedAt,
  };
}

export const useGraphQlSchemaStore = create<GraphQlSchemaState>((set, get) => {
  const put = (tabId: string, entry: SchemaEntry) =>
    set((s) => ({ entries: { ...s.entries, [tabId]: entry } }));

  return {
    entries: {},

    async loadCached(tabId, request) {
      const mine = ++counter;
      latest[tabId] = mine;
      try {
        const input = await buildSchemaRequestInput(tabId, request);
        const cached = await getCachedGraphQlSchema(input);
        if (latest[tabId] !== mine) return;
        put(tabId, cached ? ready(cached) : { status: 'idle' });
      } catch (err) {
        if (latest[tabId] !== mine) return;
        put(tabId, { status: 'error', error: message(err) });
      }
    },

    async fetchSchema(tabId, request, refresh) {
      const mine = ++counter;
      latest[tabId] = mine;
      // Keep the schema we already have, so completion and the docs survive a refresh.
      const { schema, fetchedAt } = get().entries[tabId] ?? {};
      put(tabId, { status: 'loading', schema, fetchedAt });
      try {
        const input = await buildSchemaRequestInput(tabId, request);
        const result = await fetchGraphQlSchema({ request: input, refresh });
        if (latest[tabId] !== mine) return;
        put(tabId, ready(result));
      } catch (err) {
        if (latest[tabId] !== mine) return;
        put(tabId, { status: 'error', error: message(err), schema, fetchedAt });
      }
    },

    clear(tabId) {
      set((s) => {
        const { [tabId]: _removed, ...rest } = s.entries;
        return { entries: rest };
      });
    },
  };
});
