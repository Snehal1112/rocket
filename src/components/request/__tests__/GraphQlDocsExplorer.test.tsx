import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { buildSchema } from 'graphql';
import { describe, expect, it, vi } from 'vitest';
import { GraphQlDocsExplorer } from '../GraphQlDocsExplorer';

const schema = buildSchema(`
  type Query { user(id: ID!): User }
  type User { id: ID! name: String posts: [Post!]! }
  type Post { id: ID! title: String }
`);

describe('GraphQlDocsExplorer', () => {
  it('asks the user to fetch the schema when there is none', async () => {
    const onFetch = vi.fn();
    render(<GraphQlDocsExplorer schema={undefined} status='idle' onFetch={onFetch} />);
    await userEvent.click(screen.getByRole('button', { name: /fetch schema/i }));
    expect(onFetch).toHaveBeenCalledWith(false);
  });

  it('shows the fetch error', () => {
    render(
      <GraphQlDocsExplorer
        schema={undefined}
        status='error'
        error='introspection is disabled on this server'
        onFetch={vi.fn()}
      />,
    );
    expect(screen.getByText(/introspection is disabled/)).toBeTruthy();
  });

  it('lists the root types and navigates into a field type and back', async () => {
    render(<GraphQlDocsExplorer schema={schema} status='ready' onFetch={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: /query/i }));
    expect(screen.getByText('user')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'User' }));
    expect(screen.getByText('posts')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Post' }));
    expect(screen.getByText('title')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: /back/i }));
    expect(screen.getByText('posts')).toBeTruthy();
  });

  it('filters the type list by the search text', async () => {
    render(<GraphQlDocsExplorer schema={schema} status='ready' onFetch={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText('Search types'), 'Pos');
    expect(screen.getByRole('button', { name: 'Post' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'User' })).toBeNull();
  });

  it('refreshes with the refresh flag set', async () => {
    const onFetch = vi.fn();
    render(<GraphQlDocsExplorer schema={schema} status='ready' onFetch={onFetch} />);
    await userEvent.click(screen.getByRole('button', { name: /refresh/i }));
    expect(onFetch).toHaveBeenCalledWith(true);
  });
});
