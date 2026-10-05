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

export const useGraphQlSchemaStore = create<GraphQlSchemaState>((set) => {
  const put = (tabId: string, entry: SchemaEntry) =>
    set((s) => ({ entries: { ...s.entries, [tabId]: entry } }));

  return {
    entries: {},

    async loadCached(tabId, request) {
      try {
        const input = await buildSchemaRequestInput(tabId, request);
        const cached = await getCachedGraphQlSchema(
          input.collection,
          input.environmentName,
          input.url,
        );
        put(tabId, cached ? ready(cached) : { status: 'idle' });
      } catch (err) {
        put(tabId, { status: 'error', error: message(err) });
      }
    },

    async fetchSchema(tabId, request, refresh) {
      put(tabId, { status: 'loading' });
      try {
        const input = await buildSchemaRequestInput(tabId, request);
        const result = await fetchGraphQlSchema({ request: input, refresh });
        put(tabId, ready(result));
      } catch (err) {
        put(tabId, { status: 'error', error: message(err) });
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
