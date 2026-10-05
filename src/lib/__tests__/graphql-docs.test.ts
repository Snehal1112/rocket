import { buildSchema } from 'graphql';
import { describe, expect, it } from 'vitest';
import { describeType, listTypeNames, rootTypes } from '../graphql-docs';

const schema = buildSchema(`
  """The entry point."""
  type Query {
    user(id: ID!, active: Boolean = true): User
    search(term: String!): [SearchResult!]!
    old: String @deprecated(reason: "use user")
  }
  type Mutation { rename(id: ID!, name: String!): User }
  interface Node { id: ID! }
  type User implements Node { id: ID! name: String role: Role }
  type Post implements Node { id: ID! title: String }
  union SearchResult = User | Post
  enum Role { ADMIN @deprecated(reason: "gone") MEMBER }
  input Filter { term: String! limit: Int = 10 }
  scalar DateTime
`);

describe('rootTypes', () => {
  it('lists the root operation types that exist', () => {
    expect(rootTypes(schema)).toEqual([
      { operation: 'query', typeName: 'Query' },
      { operation: 'mutation', typeName: 'Mutation' },
    ]);
  });
});

describe('listTypeNames', () => {
  it('lists named types sorted, without introspection types', () => {
    const names = listTypeNames(schema);
    expect(names).toContain('User');
    expect(names).toContain('DateTime');
    expect(names.some((n) => n.startsWith('__'))).toBe(false);
    expect([...names].sort()).toEqual(names);
  });
});

describe('describeType', () => {
  it('describes an object type with arguments, defaults and deprecation', () => {
    const q = describeType(schema, 'Query');
    expect(q?.kind).toBe('object');
    expect(q?.description).toBe('The entry point.');
    const user = q?.fields.find((f) => f.name === 'user');
    expect(user?.type).toBe('User');
    expect(user?.namedType).toBe('User');
    expect(user?.args).toEqual([
      { name: 'id', type: 'ID!', description: undefined, defaultValue: undefined },
      { name: 'active', type: 'Boolean', description: undefined, defaultValue: 'true' },
    ]);
    expect(q?.fields.find((f) => f.name === 'search')?.type).toBe('[SearchResult!]!');
    expect(q?.fields.find((f) => f.name === 'old')?.deprecation).toBe('use user');
  });

  it('describes interfaces, unions, enums, inputs and scalars', () => {
    expect(describeType(schema, 'Node')?.possibleTypes.sort()).toEqual(['Post', 'User']);
    expect(describeType(schema, 'User')?.interfaces).toEqual(['Node']);
    expect(describeType(schema, 'SearchResult')?.possibleTypes.sort()).toEqual(['Post', 'User']);
    const role = describeType(schema, 'Role');
    expect(role?.enumValues.map((v) => v.name)).toEqual(['ADMIN', 'MEMBER']);
    expect(role?.enumValues[0].deprecation).toBe('gone');
    const filter = describeType(schema, 'Filter');
    expect(filter?.kind).toBe('input');
    expect(filter?.fields.map((f) => f.name)).toEqual(['term', 'limit']);
    expect(describeType(schema, 'DateTime')?.kind).toBe('scalar');
  });

  it('returns null for an unknown type', () => {
    expect(describeType(schema, 'Nope')).toBeNull();
  });
});
