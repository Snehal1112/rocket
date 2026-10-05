import {
  type GraphQLArgument,
  type GraphQLField,
  type GraphQLInputType,
  type GraphQLObjectType,
  type GraphQLSchema,
  getNamedType,
  isEnumType,
  isInputObjectType,
  isInterfaceType,
  isListType,
  isNonNullType,
  isObjectType,
  isScalarType,
  isUnionType,
} from 'graphql';
import { DEFAULT_GRAPHQL_QUERY } from '@/lib/pane-utils';

export interface BuilderInput {
  operation: 'query' | 'mutation';
  name?: string;
  /** Dot-joined field names from the root type, such as `user.posts.title`. */
  paths: string[];
}

export interface BuilderOutput {
  query: string;
  /** A JSON skeleton of the variables, or an empty string when there are none. */
  variables: string;
  operationName?: string;
}

interface Tree {
  [field: string]: Tree;
}

interface Variable {
  name: string;
  type: GraphQLInputType;
}

const INDENT = '  ';

function buildTree(paths: string[]): Tree {
  const root: Tree = {};
  for (const path of paths) {
    let node = root;
    for (const part of path.split('.')) {
      node[part] = node[part] ?? {};
      node = node[part];
    }
  }
  return root;
}

// A required argument is non-null and has no default.
function requiredArgs(field: GraphQLField<unknown, unknown>): GraphQLArgument[] {
  return field.args.filter((a) => isNonNullType(a.type) && a.defaultValue === undefined);
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0].toUpperCase() + s.slice(1) : s;
}

// The first free name: the argument name, then `<field><Arg>`, then a numbered name.
function variableName(arg: string, field: string, taken: Set<string>): string {
  const candidates = [arg, `${field}${capitalize(arg)}`];
  for (const c of candidates) if (!taken.has(c)) return c;
  let i = 2;
  while (taken.has(`${arg}${i}`)) i += 1;
  return `${arg}${i}`;
}

function skeleton(type: GraphQLInputType): unknown {
  if (isNonNullType(type)) return skeleton(type.ofType);
  if (isListType(type)) return [];
  const named = getNamedType(type);
  if (isEnumType(named)) return named.getValues()[0]?.name ?? '';
  if (isInputObjectType(named)) return {};
  if (isScalarType(named)) {
    switch (named.name) {
      case 'Int':
      case 'Float':
        return 0;
      case 'Boolean':
        return false;
      default:
        return '';
    }
  }
  return null;
}

function emit(
  type: GraphQLObjectType,
  tree: Tree,
  depth: number,
  variables: Variable[],
  taken: Set<string>,
): string {
  const pad = INDENT.repeat(depth);
  const lines: string[] = [];
  for (const field of Object.values(type.getFields())) {
    const sub = tree[field.name];
    if (!sub) continue;

    let call = field.name;
    const args = requiredArgs(field);
    if (args.length > 0) {
      const parts = args.map((a) => {
        const name = variableName(a.name, field.name, taken);
        taken.add(name);
        variables.push({ name, type: a.type });
        return `${a.name}: $${name}`;
      });
      call += `(${parts.join(', ')})`;
    }

    const named = getNamedType(field.type);
    if (isObjectType(named)) {
      const children = emit(named, sub, depth + 1, variables, taken);
      const body = children || `${pad}${INDENT}__typename\n`;
      lines.push(`${pad}${call} {\n${body}${pad}}\n`);
    } else if (isInterfaceType(named) || isUnionType(named)) {
      // These need inline fragments, which this builder does not write.
      lines.push(`${pad}${call} {\n${pad}${INDENT}__typename\n${pad}}\n`);
    } else {
      lines.push(`${pad}${call}\n`);
    }
  }
  return lines.join('');
}

// Builds an operation from a set of selected field paths. Unknown paths are
// ignored. With nothing to select, the query is an empty string.
export function buildOperation(schema: GraphQLSchema, input: BuilderInput): BuilderOutput {
  const root = input.operation === 'query' ? schema.getQueryType() : schema.getMutationType();
  const tree = buildTree(input.paths);
  if (!root) return { query: '', variables: '' };

  const variables: Variable[] = [];
  const body = emit(root, tree, 1, variables, new Set());
  if (!body) return { query: '', variables: '' };

  const name = (input.name ?? '').replace(/[^_A-Za-z0-9]/g, '');
  const defs = variables.map((v) => `$${v.name}: ${String(v.type)}`).join(', ');
  const header = `${input.operation}${name ? ` ${name}` : ''}${defs ? `(${defs})` : ''}`;
  const skeletonObject = Object.fromEntries(variables.map((v) => [v.name, skeleton(v.type)]));

  return {
    query: `${header} {\n${body}}\n`,
    variables: variables.length > 0 ? JSON.stringify(skeletonObject, null, 2) : '',
    operationName: name || undefined,
  };
}

// The builder replaces the whole query, so a query the user wrote needs a confirmation.
export function shouldConfirmReplace(currentQuery: string): boolean {
  const trimmed = currentQuery.trim();
  return trimmed !== '' && trimmed !== DEFAULT_GRAPHQL_QUERY.trim();
}
