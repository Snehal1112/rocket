import { buildClientSchema, type GraphQLSchema, type IntrospectionQuery } from 'graphql';

// Builds a schema from the `{ __schema }` object the backend returns.
// `buildClientSchema` throws on anything that is not an introspection result.
export function buildSchemaFromIntrospection(introspection: unknown): GraphQLSchema {
  return buildClientSchema(introspection as IntrospectionQuery);
}
