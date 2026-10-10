import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { allowed, makeTrust, renderWithQuery, requestedOnly } from '@/test/trust-fixtures';
import { AgentAutonomyToggle } from '../AgentAutonomyToggle';

vi.mock('@/lib/tauri-api', () => ({
  getCollectionTrust: vi.fn(),
  setCollectionCapability: vi.fn(),
}));

import * as tauriApi from '@/lib/tauri-api';

const LABEL = 'Allow the agent to run requests in this collection';

describe('AgentAutonomyToggle', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust());
    vi.mocked(tauriApi.setCollectionCapability).mockImplementation(async (_c, _cap, enabled) =>
      makeTrust({ agentRun: enabled ? allowed : makeTrust().agentRun }),
    );
  });

  it('is off when the collection has never opted in', async () => {
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    expect(tauriApi.getCollectionTrust).toHaveBeenCalledWith('my-api');
  });

  it('is on when the collection has opted in', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ agentRun: allowed }),
    );
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it('asks before turning on and saves nothing when cancelled', async () => {
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());

    await userEvent.click(toggle);
    expect(
      await screen.findByText('Let the agent run requests in this collection?'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(tauriApi.setCollectionCapability).not.toHaveBeenCalled();
    expect(toggle).not.toBeChecked();
  });

  it('records the agent run capability once confirmed', async () => {
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());

    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole('button', { name: 'Allow' }));

    await waitFor(() =>
      expect(tauriApi.setCollectionCapability).toHaveBeenCalledWith('my-api', 'agentRun', true),
    );
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it('turns off straight away with no confirmation', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ agentRun: allowed }),
    );
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeChecked());

    await userEvent.click(toggle);

    await waitFor(() =>
      expect(tauriApi.setCollectionCapability).toHaveBeenCalledWith('my-api', 'agentRun', false),
    );
    await waitFor(() => expect(toggle).not.toBeChecked());
  });

  it('shows an error and stays off when the save fails', async () => {
    vi.mocked(tauriApi.setCollectionCapability).mockRejectedValue(new Error('disk full'));
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());

    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole('button', { name: 'Allow' }));

    expect(await screen.findByText('Failed to save this setting.')).toBeInTheDocument();
    expect(toggle).not.toBeChecked();
  });

  it('shows an error and keeps the switch disabled when loading fails', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockRejectedValue(new Error('unreadable'));
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);

    expect(await screen.findByText('Failed to load this setting.')).toBeInTheDocument();
    expect(screen.getByLabelText(LABEL)).toBeDisabled();
  });

  it('is off and says so when the file requests it but it is not allowed here', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ agentRun: requestedOnly, pending: true }),
    );
    renderWithQuery(<AgentAutonomyToggle collectionName='my-api' />);
    const toggle = await screen.findByLabelText(LABEL);
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    expect(
      screen.getByText(/files turn this on\. It is off until you allow it on this computer/),
    ).toBeInTheDocument();
  });
});
