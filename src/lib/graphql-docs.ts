import {
  type GraphQLArgument,
  type GraphQLField,
  type GraphQLInputField,
  type GraphQLNamedType,
  type GraphQLSchema,
  getNamedType,
  isEnumType,
  isInputObjectType,
  isInterfaceType,
  isObjectType,
  isScalarType,
  isUnionType,
} from 'graphql';

export interface DocsArg {
  name: string;
  type: string;
  description?: string;
  defaultValue?: string;
}

export interface DocsField {
  name: string;
  /** The full type, such as `[Post!]!`. */
  type: string;
  /** The type without list and non-null wrappers, such as `Post`. */
  namedType: string;
  description?: string;
  deprecation?: string;
  args: DocsArg[];
}

export interface DocsType {
  kind: 'object' | 'interface' | 'union' | 'enum' | 'input' | 'scalar';
  name: string;
  description?: string;
  fields: DocsField[];
  enumValues: { name: string; description?: string; deprecation?: string }[];
  /** Union members, or the implementations of an interface. */
  possibleTypes: string[];
  interfaces: string[];
}

export interface DocsRoot {
  operation: 'query' | 'mutation' | 'subscription';
  typeName: string;
}

export function rootTypes(schema: GraphQLSchema): DocsRoot[] {
  const roots: DocsRoot[] = [];
  const query = schema.getQueryType();
  const mutation = schema.getMutationType();
  const subscription = schema.getSubscriptionType();
  if (query) roots.push({ operation: 'query', typeName: query.name });
  if (mutation) roots.push({ operation: 'mutation', typeName: mutation.name });
  if (subscription) roots.push({ operation: 'subscription', typeName: subscription.name });
  return roots;
}

// Named types sorted by name. Introspection types (`__Schema` and friends) are left out.
export function listTypeNames(schema: GraphQLSchema): string[] {
  return Object.keys(schema.getTypeMap())
    .filter((n) => !n.startsWith('__'))
    .sort();
}

function toArg(a: GraphQLArgument): DocsArg {
  return {
    name: a.name,
    type: String(a.type),
    description: a.description ?? undefined,
    defaultValue: a.defaultValue !== undefined ? JSON.stringify(a.defaultValue) : undefined,
  };
}

function toField(f: GraphQLField<unknown, unknown> | GraphQLInputField): DocsField {
  const args = 'args' in f ? f.args.map(toArg) : [];
  return {
    name: f.name,
    type: String(f.type),
    namedType: getNamedType(f.type).name,
    description: f.description ?? undefined,
    deprecation: f.deprecationReason ?? undefined,
    args,
  };
}

export function describeType(schema: GraphQLSchema, name: string): DocsType | null {
  const type: GraphQLNamedType | undefined = schema.getType(name) ?? undefined;
  if (!type) return null;
  const base = {
    name: type.name,
    description: type.description ?? undefined,
    fields: [] as DocsField[],
    enumValues: [] as DocsType['enumValues'],
    possibleTypes: [] as string[],
    interfaces: [] as string[],
  };
  if (isObjectType(type)) {
    return {
      ...base,
      kind: 'object',
      fields: Object.values(type.getFields()).map(toField),
      interfaces: type.getInterfaces().map((i) => i.name),
    };
  }
  if (isInterfaceType(type)) {
    return {
      ...base,
      kind: 'interface',
      fields: Object.values(type.getFields()).map(toField),
      interfaces: type.getInterfaces().map((i) => i.name),
      possibleTypes: schema.getPossibleTypes(type).map((t) => t.name),
    };
  }
  if (isUnionType(type)) {
    return {
      ...base,
      kind: 'union',
      possibleTypes: schema.getPossibleTypes(type).map((t) => t.name),
    };
  }
  if (isEnumType(type)) {
    return {
      ...base,
      kind: 'enum',
      enumValues: type.getValues().map((v) => ({
        name: v.name,
        description: v.description ?? undefined,
        deprecation: v.deprecationReason ?? undefined,
      })),
    };
  }
  if (isInputObjectType(type)) {
    return { ...base, kind: 'input', fields: Object.values(type.getFields()).map(toField) };
  }
  if (isScalarType(type)) return { ...base, kind: 'scalar' };
  // Every kind is handled above; this is the safe answer for a future one.
  return null;
}
