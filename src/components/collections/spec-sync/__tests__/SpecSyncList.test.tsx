import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { OpenApiSyncItem } from '@/lib/tauri-api';
import { SpecSyncList } from '../SpecSyncList';
import { changedItem, makePlan } from './fixtures';

describe('SpecSyncList', () => {
  it('shows the groups with counts and calls back on select', async () => {
    const onSelect = vi.fn();
    render(<SpecSyncList plan={makePlan()} selectedId={null} onSelect={onSelect} />);
    expect(screen.getByText('New (1)')).toBeInTheDocument();
    expect(screen.getByText('Changed (1)')).toBeInTheDocument();
    expect(screen.getByText('Removed (1)')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByText('Add pet'));
    expect(onSelect).toHaveBeenCalledWith('changed:POST /pets');
  });

  it('collapses a group', async () => {
    render(<SpecSyncList plan={makePlan()} selectedId={null} onSelect={vi.fn()} />);
    const user = userEvent.setup();
    expect(screen.getByText('Add pet')).toBeInTheDocument();
    await user.click(screen.getByText('Changed (1)'));
    expect(screen.queryByText('Add pet')).toBeNull();
  });

  it('keeps the unchanged section collapsed and lists variables and notes', async () => {
    const plan = makePlan({
      unchanged: [{ key: 'GET /a', name: 'Same one' }],
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'v', spec: 'https://x' }],
      skippedHidden: 2,
      deletedByUser: ['Old request'],
    });
    render(<SpecSyncList plan={plan} selectedId={null} onSelect={vi.fn()} />);
    expect(screen.queryByText('Same one')).toBeNull();
    await userEvent.setup().click(screen.getByText('Stays as you have it (1)'));
    expect(screen.getByText('Same one')).toBeInTheDocument();
    expect(screen.getByText('Variables (1)')).toBeInTheDocument();
    expect(screen.getByText('Base URL baseUrl')).toBeInTheDocument();
    expect(screen.getByText(/2 skipped/)).toBeInTheDocument();
    expect(screen.getByText(/Old request/)).toBeInTheDocument();
  });

  it('renders a very large plan', () => {
    const items: OpenApiSyncItem[] = Array.from({ length: 3000 }, (_, i) => ({
      ...changedItem,
      key: `GET /p${i}`,
      name: `Request ${i}`,
    })) as OpenApiSyncItem[];
    render(<SpecSyncList plan={makePlan({ items })} selectedId={null} onSelect={vi.fn()} />);
    expect(screen.getByText('Changed (3000)')).toBeInTheDocument();
    expect(screen.getByText('Request 2999')).toBeInTheDocument();
  });
});
