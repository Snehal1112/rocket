import { buildSchema, introspectionFromSchema } from 'graphql';
import { describe, expect, it } from 'vitest';
import { buildSchemaFromIntrospection } from '../graphql-schema';

describe('buildSchemaFromIntrospection', () => {
  it('rebuilds a schema from an introspection result', () => {
    const source = buildSchema(
      'type Query { user(id: ID!): User } type User { id: ID! name: String }',
    );
    const schema = buildSchemaFromIntrospection(introspectionFromSchema(source));
    expect(schema.getQueryType()?.name).toBe('Query');
    expect(schema.getType('User')).toBeDefined();
  });

  it('throws a readable error for something that is not an introspection result', () => {
    expect(() => buildSchemaFromIntrospection({ nope: true })).toThrow();
  });
});
