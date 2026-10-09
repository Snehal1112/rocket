import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CollectionSettings } from '@/lib/tauri-api';
import { AgentAutonomyToggle } from '../AgentAutonomyToggle';

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(),
  saveCollectionSettings: vi.fn(),
}));

import * as tauriApi from '@/lib/tauri-api';

const LABEL = 'Allow this agent to run requests and edit files';

function settings(overrides: Partial<CollectionSettings> = {}): CollectionSettings {
  return {
    headers: [{ key: 'X-Tenant', value: 'acme', enabled: true }],
    variables: [],
    sandboxMode: 'developer',
    ...overrides,
  };
}

describe('AgentAutonomyToggle', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(settings());
    vi.mocked(tauriApi.saveCollectionSettings).mockResolvedValue(undefined);
  });

  it('is off when the collection has never opted in', async () => {
    render(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    expect(tauriApi.getCollectionSettings).toHaveBeenCalledWith('my-api');
  });

  it('is on when the collection has opted in', async () => {
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      settings({ agentAutonomyEnabled: true }),
    );
    render(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it('asks before turning on and saves nothing when cancelled', async () => {
    render(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());

    await userEvent.click(toggle);
    expect(await screen.findByText('Let the agent act in this collection?')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(tauriApi.saveCollectionSettings).not.toHaveBeenCalled();
    expect(toggle).not.toBeChecked();
  });

  it('saves the whole settings object with the flag on once confirmed', async () => {
    render(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());

    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole('button', { name: 'Allow' }));

    // The backend replaces the whole object, so the other fields must survive.
    await waitFor(() =>
      expect(tauriApi.saveCollectionSettings).toHaveBeenCalledWith(
        'my-api',
        settings({ agentAutonomyEnabled: true }),
      ),
    );
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it('turns off straight away with no confirmation', async () => {
    vi.mocked(tauriApi.getCollectionSettings).mockResolvedValue(
      settings({ agentAutonomyEnabled: true }),
    );
    render(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeChecked());

    await userEvent.click(toggle);

    await waitFor(() =>
      expect(tauriApi.saveCollectionSettings).toHaveBeenCalledWith(
        'my-api',
        settings({ agentAutonomyEnabled: false }),
      ),
    );
    await waitFor(() => expect(toggle).not.toBeChecked());
  });

  it('shows an error and stays off when the save fails', async () => {
    vi.mocked(tauriApi.saveCollectionSettings).mockRejectedValue(new Error('disk full'));
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());

    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole('button', { name: 'Allow' }));

    expect(await screen.findByText('Failed to save this setting.')).toBeInTheDocument();
    expect(toggle).not.toBeChecked();
  });

  it('shows an error and keeps the switch disabled when loading fails', async () => {
    vi.mocked(tauriApi.getCollectionSettings).mockRejectedValue(new Error('unreadable'));
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(<AgentAutonomyToggle collectionName='my-api' />);

    expect(await screen.findByText('Failed to load this setting.')).toBeInTheDocument();
    expect(screen.getByLabelText(LABEL)).toBeDisabled();
  });
});
