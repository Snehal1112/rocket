import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { AssistantPermissionsPopover } from '../AssistantPermissionsPopover';

const collections = vi.hoisted(() => ({ data: [] as Array<{ name: string }> }));

vi.mock('@/lib/queries/collection-queries', () => ({
  collectionKeys: { all: ['collections'] },
  useCollections: () => collections,
}));

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getCollectionSettings: vi.fn(),
  setCollectionCapability: vi.fn(),
}));

const LABEL = 'Allow the agent to run requests in this collection';

async function openPopover(): Promise<void> {
  await userEvent.click(screen.getByRole('button', { name: 'Agent permissions' }));
}

describe('AssistantPermissionsPopover', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collections.data = [{ name: 'orders' }, { name: 'billing' }];
    vi.mocked(api.getCollectionSettings).mockResolvedValue({
      headers: [],
      variables: [],
      sandboxMode: 'safe',
    });
    vi.mocked(api.setCollectionCapability).mockResolvedValue(undefined);
  });

  it('lists every collection with its own run switch', async () => {
    render(<AssistantPermissionsPopover />);
    await openPopover();
    const switches = await screen.findAllByRole('switch', { name: LABEL });
    expect(switches).toHaveLength(2);
    expect(new Set(switches.map((s) => s.id)).size).toBe(2);
    expect(screen.getByRole('region', { name: 'orders' })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'billing' })).toBeInTheDocument();
  });

  it('asks before letting the agent run requests in one collection', async () => {
    render(<AssistantPermissionsPopover />);
    await openPopover();
    const billing = await screen.findByRole('region', { name: 'billing' });
    const toggle = within(billing).getByRole('switch');
    await waitFor(() => expect(toggle).toBeEnabled());
    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole('button', { name: 'Allow' }));
    await waitFor(() =>
      expect(api.setCollectionCapability).toHaveBeenCalledWith('billing', 'agentRun', true),
    );
    // The confirm dialog must not have closed the popover.
    await waitFor(() =>
      expect(screen.getByRole('region', { name: 'orders' })).toBeInTheDocument(),
    );
  });

  it('says so when the workspace has no collections', async () => {
    collections.data = [];
    render(<AssistantPermissionsPopover />);
    await openPopover();
    expect(await screen.findByText('No collections in this workspace.')).toBeInTheDocument();
  });
});
