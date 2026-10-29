import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { CreateRequestDialog } from '@/components/request/CreateRequestDialog';
import { createDefaultLeaf, findTabInTree } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn(),
    saveGraphQlRequest: vi.fn(),
    saveWebSocketRequest: vi.fn(),
    createWebSocketRequest: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

describe('CreateRequestDialog websocket', () => {
  // jsdom lacks the pointer and scroll APIs the Radix Select relies on.
  beforeAll(() => {
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
    HTMLElement.prototype.setPointerCapture = vi.fn();
    HTMLElement.prototype.releasePointerCapture = vi.fn();
    HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.saveRequest).mockReset();
    vi.mocked(tauriApi.saveWebSocketRequest).mockReset();
    vi.mocked(tauriApi.createWebSocketRequest).mockReset();
    vi.mocked(toast.error).mockReset();
    vi.mocked(tauriApi.createWebSocketRequest).mockImplementation(async (_c, path, request) => ({
      ...request,
      fileName: path,
    }));
  });

  it('creates through the exclusive wrapper, not the save wrapper', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
    const option = await screen.findByRole('option', { name: 'WebSocket' });
    expect(option.getAttribute('aria-disabled')).not.toBe('true');
    await userEvent.click(option);
    await userEvent.type(screen.getByLabelText('Request Name'), 'chat');
    await userEvent.type(screen.getByLabelText('URL'), 'wss://echo.websocket.org');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.createWebSocketRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveWebSocketRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [, , payload] = vi.mocked(tauriApi.createWebSocketRequest).mock.calls[0];
    expect(payload.url).toBe('wss://echo.websocket.org');
    expect(payload.messages).toHaveLength(1);

    const found = findTabInTree(usePaneStore.getState().root, payload.uid);
    expect(found?.tab.tabType).toBe('request');
    if (found?.tab.tabType !== 'request') return;
    expect(found.tab.request.requestType).toBe('websocket');
  });

  it('shows the already exists toast and opens no tab', async () => {
    vi.mocked(tauriApi.createWebSocketRequest).mockRejectedValue(
      new Error('Already exists: col/foo.yml'),
    );
    const initialRoot = usePaneStore.getState().root;
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
    await userEvent.click(await screen.findByRole('option', { name: 'WebSocket' }));
    await userEvent.type(screen.getByLabelText('Request Name'), 'foo');
    await userEvent.type(screen.getByLabelText('URL'), 'wss://echo.websocket.org');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        'A request named "foo" already exists here. Pick another name.',
      ),
    );
    expect(usePaneStore.getState().root).toEqual(initialRoot);
  });
});
