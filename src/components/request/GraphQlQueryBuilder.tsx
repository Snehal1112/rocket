import { type GraphQLObjectType, type GraphQLSchema, getNamedType, isObjectType } from 'graphql';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  type BuilderOutput,
  buildOperation,
  shouldConfirmReplace,
} from '@/lib/graphql-query-builder';

interface GraphQlQueryBuilderProps {
  schema: GraphQLSchema;
  /** The query currently in the editor, to decide whether replacing it needs a confirmation. */
  currentQuery: string;
  onApply: (output: BuilderOutput) => void;
}

// A field tree for the query or mutation root. Ticking fields builds the query
// text; "Use in request" writes it into the request.
export function GraphQlQueryBuilder({ schema, currentQuery, onApply }: GraphQlQueryBuilderProps) {
  const [operation, setOperation] = useState<'query' | 'mutation'>('query');
  const [name, setName] = useState('');
  const [paths, setPaths] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);

  const root = operation === 'query' ? schema.getQueryType() : schema.getMutationType();
  const output = useMemo(
    () => buildOperation(schema, { operation, name, paths: [...paths] }),
    [schema, operation, name, paths],
  );

  const toggle = (path: string) =>
    setPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        // Unticking a field also unticks everything below it.
        for (const p of prev) if (p === path || p.startsWith(`${path}.`)) next.delete(p);
      } else {
        next.add(path);
      }
      return next;
    });

  const toggleExpanded = (path: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const apply = () => {
    if (shouldConfirmReplace(currentQuery)) setConfirming(true);
    else onApply(output);
  };

  const renderFields = (type: GraphQLObjectType, prefix: string, depth: number) =>
    Object.values(type.getFields()).map((field) => {
      const path = prefix ? `${prefix}.${field.name}` : field.name;
      const named = getNamedType(field.type);
      const expandable = isObjectType(named);
      const open = expanded.has(path);
      return (
        <div key={path}>
          <div className='flex items-center gap-1.5 py-0.5' style={{ paddingLeft: depth * 16 }}>
            {expandable ? (
              <Button
                variant='ghost'
                size='icon'
                className='h-5 w-5'
                aria-label={`${open ? 'Collapse' : 'Expand'} ${path}`}
                onClick={() => toggleExpanded(path)}
              >
                {open ? (
                  <ChevronDown className='h-3.5 w-3.5' />
                ) : (
                  <ChevronRight className='h-3.5 w-3.5' />
                )}
              </Button>
            ) : (
              <span className='inline-block w-5' />
            )}
            <Checkbox
              checked={paths.has(path)}
              onCheckedChange={() => toggle(path)}
              aria-label={path}
            />
            <span className='font-mono text-xs'>{field.name}</span>
            <span className='font-mono text-2xs text-muted-foreground'>{String(field.type)}</span>
          </div>
          {expandable && open && renderFields(named, path, depth + 1)}
        </div>
      );
    });

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center gap-2 border-b border-border px-3 py-1.5 shrink-0'>
        <Tabs
          value={operation}
          onValueChange={(v) => {
            setOperation(v as 'query' | 'mutation');
            setPaths(new Set());
            setExpanded(new Set());
          }}
        >
          <TabsList className='h-6'>
            <TabsTrigger value='query' className='text-[10px] px-2.5 py-0.5'>
              Query
            </TabsTrigger>
            <TabsTrigger
              value='mutation'
              className='text-[10px] px-2.5 py-0.5'
              disabled={!schema.getMutationType()}
            >
              Mutation
            </TabsTrigger>
          </TabsList>
        </Tabs>
        <Input
          className='h-7 w-40 text-xs'
          placeholder='Operation name'
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Button size='sm' className='ml-auto h-7' disabled={!output.query} onClick={apply}>
          Use in request
        </Button>
      </div>

      <div className='flex min-h-0 flex-1'>
        <ScrollArea className='flex-1 min-w-0 border-r border-border'>
          <div className='p-2'>
            {root ? (
              renderFields(root, '', 0)
            ) : (
              <p className='p-2 text-xs text-muted-foreground'>
                This schema has no {operation} root.
              </p>
            )}
          </div>
        </ScrollArea>
        <ScrollArea className='flex-1 min-w-0'>
          <pre data-testid='builder-preview' className='p-3 font-mono text-xs whitespace-pre-wrap'>
            {output.query || 'Tick fields to build a query.'}
            {output.variables ? `\n# Variables\n${output.variables}` : ''}
          </pre>
        </ScrollArea>
      </div>

      <AlertDialog open={confirming} onOpenChange={setConfirming}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Replace the query?</AlertDialogTitle>
            <AlertDialogDescription>
              The request already has a query. Using the builder replaces it and its variables.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setConfirming(false);
                onApply(output);
              }}
            >
              Replace query
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
