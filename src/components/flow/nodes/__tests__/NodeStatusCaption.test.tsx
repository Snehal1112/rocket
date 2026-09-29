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
});
