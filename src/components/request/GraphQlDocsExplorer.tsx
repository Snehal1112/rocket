import type { GraphQLSchema } from 'graphql';
import { AlertTriangle, ArrowLeft, RefreshCw } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { describeType, listTypeNames, rootTypes } from '@/lib/graphql-docs';
import type { SchemaEntry } from '@/stores/graphql-schema-store';

interface GraphQlDocsExplorerProps {
  schema: GraphQLSchema | undefined;
  status: SchemaEntry['status'];
  error?: string;
  fetchedAt?: string;
  /** `refresh` is true when the user asks to fetch again. */
  onFetch: (refresh: boolean) => void;
}

// A navigable view of the schema: root types, then any type by name.
export function GraphQlDocsExplorer({
  schema,
  status,
  error,
  fetchedAt,
  onFetch,
}: GraphQlDocsExplorerProps) {
  const [stack, setStack] = useState<string[]>([]);
  const [search, setSearch] = useState('');

  const names = useMemo(() => (schema ? listTypeNames(schema) : []), [schema]);
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return q ? names.filter((n) => n.toLowerCase().includes(q)) : [];
  }, [names, search]);

  const loading = status === 'loading';
  const current = stack.length > 0 ? stack[stack.length - 1] : undefined;
  const view = schema && current ? describeType(schema, current) : null;

  const toolbar = (
    <div className='flex items-center gap-2 border-b border-border px-3 py-1.5 shrink-0'>
      <Button
        size='sm'
        variant='outline'
        className='h-7'
        disabled={loading}
        onClick={() => onFetch(Boolean(schema))}
      >
        <RefreshCw className={`mr-1 h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
        {schema ? 'Refresh' : 'Fetch schema'}
      </Button>
      {fetchedAt && (
        <span className='text-xs text-muted-foreground'>
          Fetched {new Date(fetchedAt).toLocaleString()}
        </span>
      )}
    </div>
  );

  if (!schema) {
    return (
      <div className='flex h-full flex-col'>
        {toolbar}
        <div className='flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center'>
          {error ? (
            <Alert variant='destructive' className='max-w-md text-left'>
              <AlertTriangle className='h-4 w-4' />
              <span>{error}</span>
            </Alert>
          ) : (
            <p className='max-w-sm text-xs text-muted-foreground'>
              Fetch the schema to browse its types and get completion and validation in the query
              editor. The request uses this tab's URL, headers and auth.
            </p>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className='flex h-full flex-col'>
      {toolbar}
      <div className='flex items-center gap-2 px-3 py-2 shrink-0'>
        {stack.length > 0 && (
          <Button
            size='sm'
            variant='ghost'
            className='h-7 px-2'
            onClick={() => setStack((s) => s.slice(0, -1))}
            aria-label='Back'
          >
            <ArrowLeft className='h-3.5 w-3.5' />
          </Button>
        )}
        <Input
          className='h-7 text-xs'
          placeholder='Search types'
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>
      <ScrollArea className='flex-1 min-h-0'>
        <div className='space-y-1 px-3 pb-4'>
          {search.trim() ? (
            filtered.map((n) => (
              <Button
                key={n}
                variant='link'
                className='h-6 px-0 text-xs font-mono'
                onClick={() => {
                  setStack((s) => [...s, n]);
                  setSearch('');
                }}
              >
                {n}
              </Button>
            ))
          ) : view ? (
            <TypeView view={view} onOpen={(name) => setStack((s) => [...s, name])} />
          ) : (
            rootTypes(schema).map((r) => (
              <div key={r.operation} className='flex items-center gap-2'>
                <span className='w-20 text-xs text-muted-foreground'>{r.operation}</span>
                <Button
                  variant='link'
                  className='h-6 px-0 text-xs font-mono'
                  onClick={() => setStack([r.typeName])}
                >
                  {r.typeName}
                </Button>
              </div>
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

function TypeLink({ name, onOpen }: { name: string; onOpen: (name: string) => void }) {
  return (
    <Button variant='link' className='h-5 px-0 text-xs font-mono' onClick={() => onOpen(name)}>
      {name}
    </Button>
  );
}

function TypeView({
  view,
  onOpen,
}: {
  view: NonNullable<ReturnType<typeof describeType>>;
  onOpen: (name: string) => void;
}) {
  return (
    <div className='space-y-3 text-xs'>
      <div>
        <div className='font-mono text-sm font-semibold'>{view.name}</div>
        <div className='text-muted-foreground'>{view.kind}</div>
        {view.description && <p className='mt-1 text-muted-foreground'>{view.description}</p>}
      </div>
      {view.interfaces.length > 0 && (
        <div className='flex flex-wrap items-center gap-1'>
          <span className='text-muted-foreground'>implements</span>
          {view.interfaces.map((i) => (
            <TypeLink key={i} name={i} onOpen={onOpen} />
          ))}
        </div>
      )}
      {view.possibleTypes.length > 0 && (
        <div className='flex flex-wrap items-center gap-1'>
          <span className='text-muted-foreground'>
            {view.kind === 'union' ? 'one of' : 'implemented by'}
          </span>
          {view.possibleTypes.map((t) => (
            <TypeLink key={t} name={t} onOpen={onOpen} />
          ))}
        </div>
      )}
      {view.fields.map((f) => (
        <div key={f.name} className='space-y-0.5'>
          <div className='flex flex-wrap items-center gap-1 font-mono'>
            <span className={f.deprecation ? 'line-through' : ''}>{f.name}</span>
            {f.args.length > 0 && (
              <span className='text-muted-foreground'>
                ({f.args.map((a) => `${a.name}: ${a.type}`).join(', ')})
              </span>
            )}
            <span className='text-muted-foreground'>:</span>
            <TypeLink name={f.namedType} onOpen={onOpen} />
            <span className='text-muted-foreground'>{f.type !== f.namedType ? f.type : ''}</span>
          </div>
          {f.description && <p className='pl-2 text-muted-foreground'>{f.description}</p>}
          {f.deprecation && <p className='pl-2 text-amber-500'>Deprecated: {f.deprecation}</p>}
        </div>
      ))}
      {view.enumValues.map((v) => (
        <div key={v.name} className='font-mono'>
          <span className={v.deprecation ? 'line-through' : ''}>{v.name}</span>
          {v.description && (
            <span className='ml-2 font-sans text-muted-foreground'>{v.description}</span>
          )}
        </div>
      ))}
    </div>
  );
}
