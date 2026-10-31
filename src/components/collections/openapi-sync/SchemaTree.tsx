import type { ReactNode } from 'react';
import type { OpenApiSchemaView } from '@/lib/tauri-api';
import { SyncBadge } from './SyncBadge';

/** The type line of a node: `integer (int64)`. */
function typeText(schema: OpenApiSchemaView): string {
  return schema.format ? `${schema.kind} (${schema.format})` : schema.kind;
}

const INLINE = /(`[^`]+`|\[[^\]]+\]\([^)\s]+\))/g;

/**
 * Plain text with two light touches: `code` shows in a chip and a markdown link shows its label
 * (the address stays in the tooltip). Only React nodes are built, so no markup can come through.
 */
function Description({ text }: { text: string }) {
  const parts: ReactNode[] = text.split(INLINE).map((part, i) => {
    const key = `${i}:${part}`;
    if (part.length > 2 && part.startsWith('`') && part.endsWith('`')) {
      return (
        <code
          key={key}
          className='rounded bg-muted px-1 py-0.5 font-mono text-[11px] text-foreground'
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(part);
    if (link) {
      return (
        <span key={key} title={link[2]} className='text-foreground underline decoration-dotted'>
          {link[1]}
        </span>
      );
    }
    return part;
  });
  return (
    <p className='max-w-prose font-[ui-sans-serif,system-ui,sans-serif] text-[13px] leading-relaxed text-muted-foreground'>
      {parts}
    </p>
  );
}

/**
 * A schema as a nested list: a name and type line, then the description under it. A guide line
 * on the left shows the nesting. Every text is a React text node, so a description from the
 * spec cannot add markup.
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
    <div
      className={
        depth === 0
          ? 'flex flex-col gap-3'
          : 'ml-1 flex flex-col gap-1.5 border-l border-border pl-4'
      }
    >
      <div className='flex flex-wrap items-center gap-x-2 gap-y-1'>
        {name && <span className='font-mono text-[13px] font-semibold'>{name}</span>}
        <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground'>
          {typeText(schema)}
        </span>
        {schema.required && <SyncBadge tone='warning'>required</SyncBadge>}
        {schema.nullable && <SyncBadge tone='neutral'>nullable</SyncBadge>}
      </div>
      {schema.description && <Description text={schema.description} />}
      {schema.refName && <p className='text-muted-foreground'>{`Same as ${schema.refName}`}</p>}
      {schema.enumValues.length > 0 && (
        <p className='font-mono text-[11px] text-muted-foreground'>
          {schema.enumValues.join(', ')}
        </p>
      )}
      {schema.properties.map((p) => (
        <SchemaTree key={p.name} name={p.name} schema={p.schema} depth={depth + 1} />
      ))}
      {schema.items && (
        <>
          <p className='text-muted-foreground'>items</p>
          <SchemaTree schema={schema.items} depth={depth + 1} />
        </>
      )}
      {schema.variants.length > 0 && (
        <>
          <p className='text-muted-foreground'>one of</p>
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
