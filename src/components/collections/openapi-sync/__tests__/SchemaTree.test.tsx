import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { OpenApiSchemaView } from '@/lib/tauri-api';
import { SchemaTree } from '../SchemaTree';

function node(over: Partial<OpenApiSchemaView> = {}): OpenApiSchemaView {
  return {
    kind: 'string',
    format: null,
    description: null,
    required: false,
    nullable: false,
    enumValues: [],
    refName: null,
    properties: [],
    items: null,
    variants: [],
    cut: false,
    ...over,
  };
}

describe('SchemaTree', () => {
  it('shows_name_type_format_required_and_nullable', () => {
    render(
      <SchemaTree
        name='id'
        schema={node({ kind: 'integer', format: 'int64', required: true, nullable: true })}
      />,
    );
    expect(screen.getByText('id')).toBeInTheDocument();
    expect(screen.getByText('integer (int64)')).toBeInTheDocument();
    expect(screen.getByText('required')).toBeInTheDocument();
    expect(screen.getByText('nullable')).toBeInTheDocument();
  });

  it('schema_text_is_rendered_as_plain_text', () => {
    const { container } = render(
      <SchemaTree schema={node({ description: '<img src=x onerror=alert(1)><b>bold</b>' })} />,
    );
    expect(screen.getByText('<img src=x onerror=alert(1)><b>bold</b>')).toBeInTheDocument();
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('b')).toBeNull();
  });

  it('a_cut_schema_says_it_is_partial', () => {
    render(<SchemaTree schema={node({ kind: 'object', cut: true })} />);
    expect(screen.getByText('Shown partly, the schema is large.')).toBeInTheDocument();
  });

  it('a_cycle_shows_same_as_reference', () => {
    render(<SchemaTree name='next' schema={node({ kind: 'any', refName: 'Node' })} />);
    expect(screen.getByText('Same as Node')).toBeInTheDocument();
  });

  it('enum_values_items_and_variants_are_shown', () => {
    render(
      <SchemaTree
        schema={node({
          kind: 'array',
          items: node({ enumValues: ['a', 'b'] }),
          variants: [node({ kind: 'integer' })],
        })}
      />,
    );
    expect(screen.getByText('a, b')).toBeInTheDocument();
    expect(screen.getByText('items')).toBeInTheDocument();
    expect(screen.getByText('one of')).toBeInTheDocument();
  });
});
