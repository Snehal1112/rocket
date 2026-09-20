import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { CommitDiffView } from '@/components/git/CommitDiffView';

describe('CommitDiffView file row', () => {
  it('selects a file and shows its diff when clicked', async () => {
    render(
      <CommitDiffView
        diffs={[
          { path: 'a.txt', oldContent: 'old-a', newContent: 'new-a', hunks: [] },
          { path: 'b.txt', oldContent: 'old-b', newContent: 'new-b', hunks: [] },
        ]}
        repositoryId='repo-1'
        repositoryLabel='Repo'
      />,
    );
    const user = userEvent.setup();

    expect(screen.getByRole('button', { name: /a\.txt/ })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /b\.txt/ }));
    expect(screen.getByRole('button', { name: /b\.txt/ })).toHaveClass('bg-muted/70');
  });
});
