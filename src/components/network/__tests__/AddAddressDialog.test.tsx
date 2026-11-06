import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AddAddressDialog } from '@/components/network/AddAddressDialog';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    addNetworkAddress: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

function setup() {
  const onOpenChange = vi.fn();
  render(<AddAddressDialog open onOpenChange={onOpenChange} />);
  return { onOpenChange };
}

const host = () => screen.getByLabelText('Host');
const port = () => screen.getByLabelText('Port');

describe('AddAddressDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.addNetworkAddress.mockResolvedValue(undefined);
    mocks.api.listNetworkNodes.mockResolvedValue([]);
  });

  it('starts with the default port and an empty host', () => {
    setup();
    expect(host()).toHaveValue('');
    expect(port()).toHaveValue('47821');
  });

  it('adds the address, reloads the nodes and closes', async () => {
    const { onOpenChange } = setup();
    await userEvent.type(host(), ' 10.0.0.5 ');
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    await waitFor(() => expect(mocks.api.addNetworkAddress).toHaveBeenCalledWith('10.0.0.5', 47821));
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalled());
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('submits with Enter in the port field', async () => {
    setup();
    await userEvent.type(host(), 'mac.tail1234.ts.net');
    await userEvent.clear(port());
    await userEvent.type(port(), '47822{Enter}');
    await waitFor(() =>
      expect(mocks.api.addNetworkAddress).toHaveBeenCalledWith('mac.tail1234.ts.net', 47822),
    );
  });

  it('refuses an empty host without calling the backend', async () => {
    const { onOpenChange } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a host name or IP address.');
    expect(mocks.api.addNetworkAddress).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it.each(['0', '65536', '80a'])('refuses the port %s', async (value) => {
    setup();
    await userEvent.type(host(), 'host');
    await userEvent.clear(port());
    await userEvent.type(port(), value);
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Port must be a number from 1 to 65535.');
    expect(mocks.api.addNetworkAddress).not.toHaveBeenCalled();
  });

  it('shows the backend error and stays open', async () => {
    mocks.api.addNetworkAddress.mockRejectedValue('Invalid input: You already saved 50 addresses.');
    const { onOpenChange } = setup();
    await userEvent.type(host(), 'host');
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('You already saved 50 addresses.');
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Add' })).toBeEnabled();
  });
});
