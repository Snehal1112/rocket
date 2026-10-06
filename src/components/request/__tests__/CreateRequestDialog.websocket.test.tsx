import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
  };
});

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
    vi.mocked(tauriApi.saveWebSocketRequest).mockImplementation(async (_c, path, request) => ({
      ...request,
      fileName: path,
    }));
  });

  it('saves a real WebSocket item and opens it, instead of an HTTP file under a WebSocket label', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
    const option = await screen.findByRole('option', { name: 'WebSocket' });
    expect(option.getAttribute('aria-disabled')).not.toBe('true');
    await userEvent.click(option);
    await userEvent.type(screen.getByLabelText('Request Name'), 'chat');
    await userEvent.type(screen.getByLabelText('URL'), 'wss://echo.websocket.org');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.saveWebSocketRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [, , payload] = vi.mocked(tauriApi.saveWebSocketRequest).mock.calls[0];
    expect(payload.url).toBe('wss://echo.websocket.org');
    expect(payload.messages).toHaveLength(1);

    const found = findTabInTree(usePaneStore.getState().root, payload.uid);
    expect(found?.tab.tabType).toBe('request');
    if (found?.tab.tabType !== 'request') return;
    expect(found.tab.request.requestType).toBe('websocket');
  });
});
