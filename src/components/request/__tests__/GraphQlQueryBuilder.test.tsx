import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { buildSchema } from 'graphql';
import { describe, expect, it, vi } from 'vitest';
import { GraphQlQueryBuilder } from '../GraphQlQueryBuilder';

const schema = buildSchema(`
  type Query { user(id: ID!): User ping: String }
  type User { id: ID! name: String }
`);

describe('GraphQlQueryBuilder', () => {
  it('previews the query for the ticked fields', async () => {
    render(<GraphQlQueryBuilder schema={schema} currentQuery='' onApply={vi.fn()} />);
    await userEvent.click(screen.getByRole('checkbox', { name: 'ping' }));
    expect(screen.getByTestId('builder-preview').textContent).toContain('ping');
  });

  it('expands an object field and ticks a nested field', async () => {
    render(<GraphQlQueryBuilder schema={schema} currentQuery='' onApply={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Expand user' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'user.name' }));
    const preview = screen.getByTestId('builder-preview').textContent ?? '';
    expect(preview).toContain('user(id: $id)');
    expect(preview).toContain('name');
  });

  it('applies straight away when the current query is the default', async () => {
    const onApply = vi.fn();
    render(
      <GraphQlQueryBuilder
        schema={schema}
        currentQuery={'{\n  __typename\n}\n'}
        onApply={onApply}
      />,
    );
    await userEvent.click(screen.getByRole('checkbox', { name: 'ping' }));
    await userEvent.click(screen.getByRole('button', { name: 'Use in request' }));
    expect(onApply).toHaveBeenCalledTimes(1);
    expect(onApply.mock.calls[0][0].query).toContain('ping');
  });

  it('asks before replacing a query the user wrote', async () => {
    const onApply = vi.fn();
    render(
      <GraphQlQueryBuilder schema={schema} currentQuery='{ users { id } }' onApply={onApply} />,
    );
    await userEvent.click(screen.getByRole('checkbox', { name: 'ping' }));
    await userEvent.click(screen.getByRole('button', { name: 'Use in request' }));
    expect(onApply).not.toHaveBeenCalled();
    await userEvent.click(await screen.findByRole('button', { name: 'Replace query' }));
    await waitFor(() => expect(onApply).toHaveBeenCalledTimes(1));
  });

  it('disables Use in request until a field is ticked', () => {
    render(<GraphQlQueryBuilder schema={schema} currentQuery='' onApply={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Use in request' }).hasAttribute('disabled')).toBe(
      true,
    );
  });
});
