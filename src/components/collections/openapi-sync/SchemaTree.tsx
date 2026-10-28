import type { OpenApiSchemaView } from '@/lib/tauri-api';
import { SyncBadge } from './SyncBadge';

/** The type line of a node: `integer (int64)`. */
function typeText(schema: OpenApiSchemaView): string {
  return schema.format ? `${schema.kind} (${schema.format})` : schema.kind;
}

/**
 * A schema as an indented list of plain text. Every text is a React text node, so a
 * description from the spec cannot add markup.
 */
export function SchemaTree({
  schema,
  name,
  depth = 0,
}: {
  schema: OpenApiSchemaView;
  name?: string;
  depth?: number;
}) {
  return (
    <div className={depth === 0 ? 'flex flex-col gap-0.5' : 'ml-4 flex flex-col gap-0.5'}>
      <div className='flex flex-wrap items-center gap-1.5 font-mono'>
        {name && <span className='font-semibold'>{name}</span>}
        <span className='text-muted-foreground'>{typeText(schema)}</span>
        {schema.required && <SyncBadge tone='warning'>required</SyncBadge>}
        {schema.nullable && <SyncBadge tone='neutral'>nullable</SyncBadge>}
      </div>
      {schema.refName && <p className='text-muted-foreground'>{`Same as ${schema.refName}`}</p>}
      {schema.enumValues.length > 0 && (
        <p className='font-mono text-muted-foreground'>{schema.enumValues.join(', ')}</p>
      )}
      {schema.description && <p className='text-muted-foreground'>{schema.description}</p>}
      {schema.properties.map((p) => (
        <SchemaTree key={p.name} name={p.name} schema={p.schema} depth={depth + 1} />
      ))}
      {schema.items && (
        <>
          <p className='ml-4 text-muted-foreground'>items</p>
          <SchemaTree schema={schema.items} depth={depth + 1} />
        </>
      )}
      {schema.variants.length > 0 && (
        <>
          <p className='ml-4 text-muted-foreground'>one of</p>
          {schema.variants.map((v, i) => (
            // The variants have no names and never reorder.
            // biome-ignore lint/suspicious/noArrayIndexKey: stable position
            <SchemaTree key={i} schema={v} depth={depth + 1} />
          ))}
        </>
      )}
      {schema.cut && <p className='text-muted-foreground'>Shown partly, the schema is large.</p>}
    </div>
  );
}
