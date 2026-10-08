import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FlowNodeStatus } from '@/lib/tauri-api';
import { NodeStatusIcon } from '../NodeStatusIcon';

describe('NodeStatusIcon', () => {
  it('renders nothing for an idle node', () => {
    const { container } = render(<NodeStatusIcon status='idle' />);
    expect(container).toBeEmptyDOMElement();
  });

  it.each<FlowNodeStatus>(['running', 'success', 'failed', 'skipped'])(
    'renders a hidden icon for %s',
    (status) => {
      render(<NodeStatusIcon status={status} />);
      const icon = screen.getByTestId('node-status-icon');
      expect(icon).toHaveAttribute('data-status', status);
      expect(icon).toHaveAttribute('aria-hidden', 'true');
      expect(icon.textContent).toBe('');
      expect(icon.querySelector('svg')).not.toBeNull();
    },
  );

  it('spins only while running, and not for users who prefer reduced motion', () => {
    const { rerender } = render(<NodeStatusIcon status='running' />);
    const cls = screen.getByTestId('node-status-icon').querySelector('svg')?.getAttribute('class');
    expect(cls).toContain('animate-spin');
    expect(cls).toContain('motion-reduce:animate-none');
    rerender(<NodeStatusIcon status='success' />);
    expect(screen.getByTestId('node-status-icon').querySelector('svg')?.getAttribute('class')).not.toContain(
      'animate-spin',
    );
  });
});
