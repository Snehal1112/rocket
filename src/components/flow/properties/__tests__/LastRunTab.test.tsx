import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { LastRunTab } from '../LastRunTab';

// Monaco cannot run in jsdom. A read-only textarea stands in for it.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: { value: string }) => (
    <textarea aria-label='Body viewer' readOnly value={props.value} />
  ),
}));

const node = (kind: FlowNodeKind): FlowNode => ({ id: 'n1', kind, position: { x: 0, y: 0 } });
const request = node({
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'auth/login.yml' },
});

describe('LastRunTab status', () => {
  it('shows the never-ran state', () => {
    render(<LastRunTab node={request} status='idle' />);
    expect(screen.getByText('Not run yet. Run the flow to see results here.')).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-status')).not.toBeInTheDocument();
  });

  it('shows the status line of a success', () => {
    render(
      <LastRunTab node={request} status='success' detail={{ statusCode: 200, durationMs: 184 }} />,
    );
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('Success');
    expect(line).toHaveTextContent('200');
    expect(line).toHaveTextContent('184ms');
  });

  it('shows attempts and total time for a polled request', () => {
    render(
      <LastRunTab
        node={request}
        status='success'
        detail={{ statusCode: 200, durationMs: 14200, attempts: 7 }}
      />,
    );
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('7 attempts');
    expect(line).toHaveTextContent('14.2s');
  });

  it('shows progress while running', () => {
    render(<LastRunTab node={request} status='running' detail={{ progress: 'attempt 3/30' }} />);
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('Running');
    expect(line).toHaveTextContent('attempt 3/30');
  });

  it('shows the full error text', () => {
    const error = `condition not met after 30 attempts (60.0s) ${'x'.repeat(400)}`;
    render(<LastRunTab node={request} status='failed' detail={{ statusCode: 404, error }} />);
    const box = screen.getByTestId('last-run-error');
    expect(box).toHaveTextContent(error);
    expect(box.className).not.toContain('line-clamp');
    expect(box.className).toContain('select-text');
  });

  it('explains an upstream skip', () => {
    render(
      <LastRunTab node={request} status='skipped' detail={{ skipReason: 'upstream_failed' }} />,
    );
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('Skipped');
    expect(screen.getByText('An earlier node failed.')).toBeInTheDocument();
  });

  it('explains a branch that was not taken', () => {
    render(
      <LastRunTab node={request} status='skipped' detail={{ skipReason: 'branch_not_taken' }} />,
    );
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('Not taken');
    expect(screen.getByText('Its branch was not taken.')).toBeInTheDocument();
  });
});
