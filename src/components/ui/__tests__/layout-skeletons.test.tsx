import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionOverviewSkeleton } from '@/components/collections/CollectionOverviewSkeleton';
import { CollectionTreeSkeleton } from '@/components/collections/CollectionTreeSkeleton';
import { PanelSkeleton } from '@/components/panes/PanelSkeleton';
import { WorkspaceCollectionsSkeleton } from '@/components/workspace/WorkspaceCollectionsSkeleton';

// Each skeleton names what loads for screen readers, and draws its shapes after the delay.
const CASES = [
  { name: 'CollectionTreeSkeleton', node: <CollectionTreeSkeleton />, label: 'Loading collections' },
  {
    name: 'CollectionOverviewSkeleton',
    node: <CollectionOverviewSkeleton />,
    label: 'Loading collection',
  },
  {
    name: 'WorkspaceCollectionsSkeleton',
    node: <WorkspaceCollectionsSkeleton />,
    label: 'Loading collections',
  },
  { name: 'PanelSkeleton', node: <PanelSkeleton label='Loading runner' />, label: 'Loading runner' },
];

describe('layout skeletons', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  for (const { name, node, label } of CASES) {
    it(`${name} announces "${label}" and shows shapes after the delay`, () => {
      const { container } = render(node);
      expect(screen.getByRole('status')).toHaveTextContent(label);
      expect(container.querySelectorAll('.bg-muted')).toHaveLength(0);
      act(() => {
        vi.advanceTimersByTime(150);
      });
      expect(container.querySelectorAll('.bg-muted').length).toBeGreaterThan(0);
    });
  }

  it('the tree skeleton uses the tree row height and indent guide', () => {
    const { container } = render(<CollectionTreeSkeleton />);
    act(() => {
      vi.advanceTimersByTime(150);
    });
    expect(container.querySelector('.h-7')).not.toBeNull();
    expect(container.querySelector('.ml-\\[14px\\].border-l')).not.toBeNull();
  });
});
