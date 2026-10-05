import { buildSchema } from 'graphql';
import { describe, expect, it } from 'vitest';
import { buildOperation, shouldConfirmReplace } from '../graphql-query-builder';

const schema = buildSchema(`
  type Query {
    me: User
    user(id: ID!, verbose: Boolean = false): User
    post(id: ID!): Post
    search(term: String!, limit: Int): [Result!]!
    ping: String
  }
  type Mutation { rename(id: ID!, name: String!): User }
  type User { id: ID! name: String posts(first: Int!): [Post!]! role: Role }
  type Post { id: ID! title: String author: User }
  union Result = User | Post
  enum Role { ADMIN MEMBER }
`);

describe('buildOperation', () => {
  it('writes a plain selection for a field without arguments', () => {
    const out = buildOperation(schema, {
      operation: 'query',
      name: 'Me',
      paths: ['me.id', 'me.name'],
    });
    expect(out.query).toBe('query Me {\n  me {\n    id\n    name\n  }\n}\n');
    expect(out.variables).toBe('');
    expect(out.operationName).toBe('Me');
  });

  it('turns a required argument into a typed variable and a variables skeleton', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'U', paths: ['user.name'] });
    expect(out.query).toBe('query U($id: ID!) {\n  user(id: $id) {\n    name\n  }\n}\n');
    expect(JSON.parse(out.variables)).toEqual({ id: '' });
  });

  it('skips optional arguments and arguments with defaults', () => {
    const out = buildOperation(schema, {
      operation: 'query',
      name: 'S',
      paths: ['search.__typename'],
    });
    expect(out.query).toContain('search(term: $term)');
    expect(out.query).not.toContain('limit');
    expect(out.query).not.toContain('verbose');
  });

  it('gives colliding variable names a distinct name', () => {
    const out = buildOperation(schema, {
      operation: 'query',
      name: 'Both',
      paths: ['user.name', 'post.title'],
    });
    expect(out.query).toContain('$id: ID!');
    expect(out.query).toContain('$postId: ID!');
    expect(out.query).toContain('user(id: $id)');
    expect(out.query).toContain('post(id: $postId)');
    expect(Object.keys(JSON.parse(out.variables)).sort()).toEqual(['id', 'postId']);
  });

  it('handles a required argument on a nested field', () => {
    const out = buildOperation(schema, {
      operation: 'query',
      name: 'N',
      paths: ['me.posts.title'],
    });
    expect(out.query).toContain('query N($first: Int!)');
    expect(out.query).toContain('posts(first: $first)');
    expect(JSON.parse(out.variables)).toEqual({ first: 0 });
  });

  it('never leaves an object field with an empty selection set', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'E', paths: ['me'] });
    expect(out.query).toBe('query E {\n  me {\n    __typename\n  }\n}\n');
  });

  it('selects only __typename on a union field', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'R', paths: ['search'] });
    expect(out.query).toContain('search(term: $term) {\n    __typename\n  }');
  });

  it('builds a mutation from the mutation root', () => {
    const out = buildOperation(schema, {
      operation: 'mutation',
      name: 'Rename',
      paths: ['rename.id'],
    });
    expect(out.query).toBe(
      'mutation Rename($id: ID!, $name: String!) {\n  rename(id: $id, name: $name) {\n    id\n  }\n}\n',
    );
    expect(JSON.parse(out.variables)).toEqual({ id: '', name: '' });
  });

  it('fills enum variables with the first value', () => {
    const s = buildSchema('type Query { byRole(role: Role!): String } enum Role { ADMIN MEMBER }');
    const out = buildOperation(s, { operation: 'query', name: 'R', paths: ['byRole'] });
    expect(JSON.parse(out.variables)).toEqual({ role: 'ADMIN' });
  });

  it('ignores paths that are not in the schema and returns an empty query for no paths', () => {
    expect(buildOperation(schema, { operation: 'query', name: 'X', paths: ['nope.x'] }).query).toBe(
      '',
    );
    expect(buildOperation(schema, { operation: 'query', name: 'X', paths: [] }).query).toBe('');
  });

  it('sanitises the operation name and omits it when nothing is left', () => {
    const named = buildOperation(schema, {
      operation: 'query',
      name: 'my query!',
      paths: ['ping'],
    });
    expect(named.query.startsWith('query myquery {')).toBe(true);
    const anon = buildOperation(schema, { operation: 'query', name: '!!', paths: ['ping'] });
    expect(anon.query.startsWith('query {')).toBe(true);
    expect(anon.operationName).toBeUndefined();
  });

  it('drops leading digits so the operation name stays valid', () => {
    const out = buildOperation(schema, { operation: 'query', name: '1users', paths: ['ping'] });
    expect(out.query.startsWith('query users {')).toBe(true);
    expect(out.operationName).toBe('users');
  });

  it('returns an empty query when the schema has no mutation root', () => {
    const s = buildSchema('type Query { a: String }');
    expect(buildOperation(s, { operation: 'mutation', name: 'M', paths: ['a'] }).query).toBe('');
  });
});

describe('shouldConfirmReplace', () => {
  it('does not ask before replacing an empty or default query', () => {
    expect(shouldConfirmReplace('')).toBe(false);
    expect(shouldConfirmReplace('  \n')).toBe(false);
    expect(shouldConfirmReplace('{\n  __typename\n}\n')).toBe(false);
  });

  it('asks before replacing a query the user wrote', () => {
    expect(shouldConfirmReplace('{ users { id } }')).toBe(true);
  });
});
