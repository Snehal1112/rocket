import { describe, expect, it } from 'vitest';
import type { GraphQlRequest } from '@/lib/tauri-api';
import { createDefaultRequestFor, DEFAULT_GRAPHQL_QUERY, mapGraphQlToState } from '../pane-utils';

const saved: GraphQlRequest = {
  uid: 'g1',
  name: 'Users',
  method: 'POST',
  url: 'https://api.example.com/graphql',
  headers: [{ key: 'X-Trace', value: '1', enabled: true }],
  auth: { authType: 'bearer', token: 't' } as GraphQlRequest['auth'],
  body: { query: '{ users { id } }', variables: '{"n":1}' },
  bodyVariants: [
    { title: 'A', selected: true, body: { query: '{ users { id } }', variables: '{"n":1}' } },
    { title: 'B', selected: false, body: { query: '{ b }' } },
  ],
  settings: { timeout: 5000 },
  preRequestScript: '// pre',
  tags: ['smoke'],
};

describe('mapGraphQlToState', () => {
  it('marks the tab graphql and carries the query, variables and variants', () => {
    const state = mapGraphQlToState(saved);
    expect(state.requestType).toBe('graphql');
    expect(state.method).toBe('POST');
    expect(state.graphql?.query).toBe('{ users { id } }');
    expect(state.graphql?.variables).toBe('{"n":1}');
    expect(state.graphql?.bodyVariants).toHaveLength(2);
  });

  it('reuses the HTTP mapping for headers, auth, scripts, tags and settings', () => {
    const state = mapGraphQlToState(saved);
    expect(state.headers[0]).toMatchObject({ key: 'X-Trace', value: '1', enabled: true });
    expect(state.auth.authType).toBe('bearer');
    expect(state.preRequestScript).toBe('// pre');
    expect(state.tags).toEqual(['smoke']);
    expect(state.settings.timeoutMs).toBe(5000);
  });

  it('defaults a missing variables field to an empty string', () => {
    const state = mapGraphQlToState({
      ...saved,
      body: { query: '{ a }' },
      bodyVariants: undefined,
    });
    expect(state.graphql?.variables).toBe('');
    expect(state.graphql?.bodyVariants).toBeUndefined();
  });
});

describe('createDefaultRequestFor', () => {
  it('builds a POST graphql request with the default query', () => {
    const state = createDefaultRequestFor('graphql');
    expect(state.requestType).toBe('graphql');
    expect(state.method).toBe('POST');
    expect(state.graphql).toEqual({ query: DEFAULT_GRAPHQL_QUERY, variables: '' });
  });

  it('keeps the existing behaviour for the other kinds', () => {
    expect(createDefaultRequestFor('http').requestType).toBe('http');
    expect(createDefaultRequestFor('grpc').requestType).toBe('grpc');
    expect(createDefaultRequestFor('http').graphql).toBeUndefined();
  });
});
