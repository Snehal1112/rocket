import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { NodeStatusCaption } from '../NodeStatusCaption';

describe('NodeStatusCaption', () => {
  it('shows progress while the node is running', () => {
    render(<NodeStatusCaption status='running' progress='attempt 3/30' />);
    expect(screen.getByTestId('node-progress')).toHaveTextContent('attempt 3/30');
  });

  it('shows nothing for a running node without progress', () => {
    const { container } = render(<NodeStatusCaption status='running' />);
    expect(container).toBeEmptyDOMElement();
  });

  it('hides progress once the node is no longer running', () => {
    render(<NodeStatusCaption status='success' progress='attempt 3/30' />);
    expect(screen.queryByTestId('node-progress')).toBeNull();
  });

  it('says a success is from an earlier run while a partial run is in progress', () => {
    render(<NodeStatusCaption status='success' cached />);
    expect(screen.getByTestId('node-cached-caption')).toHaveTextContent(
      'Result from an earlier run',
    );
  });

  it('adds the earlier-run note to a skip caption', () => {
    render(<NodeStatusCaption status='skipped' skipReason='branch_not_taken' cached />);
    expect(screen.getByTestId('node-cached-caption')).toHaveTextContent(/from an earlier run/);
  });

  it('keeps showing the error of a failed node', () => {
    render(<NodeStatusCaption status='failed' error='boom' cached />);
    expect(screen.getByTestId('node-error')).toHaveTextContent('boom');
  });
});
