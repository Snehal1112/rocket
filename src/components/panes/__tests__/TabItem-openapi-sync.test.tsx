import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { OpenApiSyncTab } from '@/types/pane-types';
import { TabItem } from '../TabItem';

const tab: OpenApiSyncTab = {
  id: 'openapi-sync:Orders',
  title: 'OpenAPI sync — Orders',
  isDirty: false,
  tabType: 'openapi-sync',
  collectionName: 'Orders',
};

describe('TabItem OpenAPI sync tab', () => {
  it('shows_the_branch_compare_icon_and_the_title', () => {
    const { container } = render(
      <TabItem tab={tab} isActive fromGroupId='g1' onSelect={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByText('OpenAPI sync — Orders')).toBeInTheDocument();
    expect(container.querySelector('svg.lucide-git-compare-arrows')).not.toBeNull();
    expect(container.querySelector('svg.lucide-refresh-cw')).toBeNull();
  });
});
