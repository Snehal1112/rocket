import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GraphQlEditor } from '../GraphQlEditor';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ language, value }: { language?: string; value: string }) => (
    <div data-testid={`monaco-${language}`}>{value}</div>
  ),
}));

vi.mock('@/lib/tauri-api', () => ({
  listGraphQlOperations: vi.fn(),
}));

import { listGraphQlOperations } from '@/lib/tauri-api';

describe('GraphQlEditor', () => {
  beforeEach(() => {
    vi.mocked(listGraphQlOperations).mockReset();
  });

  it('shows the query in a graphql editor and the variables in a json editor', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([{ name: 'A', kind: 'query' }]);
    render(
      <GraphQlEditor
        state={{ query: 'query A { a }', variables: '{"n": 1}' }}
        onChange={vi.fn()}
      />,
    );
    expect(await screen.findByTestId('monaco-graphql')).toHaveTextContent('query A { a }');
    expect(await screen.findByTestId('monaco-json')).toHaveTextContent('{"n": 1}');
  });

  it('selects the first named operation when the document has several', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([
      { name: 'A', kind: 'query' },
      { name: 'B', kind: 'mutation' },
    ]);
    const onChange = vi.fn();
    render(
      <GraphQlEditor
        state={{ query: 'query A { a } mutation B { b }', variables: '' }}
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(onChange).toHaveBeenCalledWith({ operationName: 'A' }));
  });

  it('does not touch the operation for a single-operation document', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([{ name: 'A', kind: 'query' }]);
    const onChange = vi.fn();
    render(<GraphQlEditor state={{ query: 'query A { a }', variables: '' }} onChange={onChange} />);
    await screen.findByTestId('monaco-graphql');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('shows an inline error for variables that are not a JSON object', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([]);
    render(<GraphQlEditor state={{ query: '{ a }', variables: '[1]' }} onChange={vi.fn()} />);
    expect(await screen.findByText('Variables must be a JSON object.')).toBeTruthy();
  });
});
