import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import { NodeIssueBadge } from '../NodeIssueBadge';

const error: FlowIssue = { code: 'expr-blank', severity: 'error', nodeId: 'n1', message: 'Boom.' };
const warning: FlowIssue = { code: 'exit-unwired', severity: 'warning', nodeId: 'n1', message: 'Careful.' };

describe('NodeIssueBadge', () => {
  it('renders nothing without issues', () => {
    const { container, rerender } = render(<NodeIssueBadge issues={undefined} />);
    expect(container).toBeEmptyDOMElement();
    rerender(<NodeIssueBadge issues={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('names a single error and marks its severity', () => {
    render(<NodeIssueBadge issues={[error]} />);
    const badge = screen.getByRole('img', { name: 'Error: Boom.' });
    expect(badge).toHaveAttribute('data-severity', 'error');
  });

  it('shows the worst severity and names every issue', () => {
    render(<NodeIssueBadge issues={[warning, error]} />);
    const badge = screen.getByTestId('node-issue-badge');
    expect(badge).toHaveAttribute('data-severity', 'error');
    expect(badge).toHaveAccessibleName('2 issues: Warning: Careful. Error: Boom.');
  });

  it('adds no text to the node card, only an icon', () => {
    render(<NodeIssueBadge issues={[error, warning]} />);
    expect(screen.getByTestId('node-issue-badge').textContent).toBe('');
  });
});
