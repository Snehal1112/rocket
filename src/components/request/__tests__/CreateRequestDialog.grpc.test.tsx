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
    saveGrpcRequest: vi.fn(),
  };
});

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
beforeAll(() => {
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

async function chooseGrpc() {
  await userEvent.click(screen.getByLabelText('Request Type'));
  await userEvent.click(await screen.findByRole('option', { name: 'gRPC' }));
}

describe('CreateRequestDialog grpc', () => {
  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.saveRequest).mockReset();
    vi.mocked(tauriApi.saveGrpcRequest).mockReset();
  });

  it('offers gRPC as a selectable type', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);
    await userEvent.click(screen.getByLabelText('Request Type'));
    const option = await screen.findByRole('option', { name: 'gRPC' });
    expect(option.getAttribute('aria-disabled')).not.toBe('true');
  });

  it('saves a real gRPC item, not an HTTP request tagged grpc', async () => {
    vi.mocked(tauriApi.saveGrpcRequest).mockImplementation(async (_c, _p, request) => ({
      ...request,
      fileName: 'say-hello.yml',
    }));
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await chooseGrpc();
    await userEvent.type(screen.getByLabelText('Request Name'), 'say hello');
    await userEvent.type(screen.getByLabelText('URL'), 'localhost:50051');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.saveGrpcRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [collection, , payload] = vi.mocked(tauriApi.saveGrpcRequest).mock.calls[0];
    expect(collection).toBe('api');
    expect(payload.url).toBe('localhost:50051');
    expect(payload.methodType).toBe('unary');
    expect(payload.messages).toEqual([{ title: '', selected: true, content: '{}' }]);

    const tab = findTabInTree(usePaneStore.getState().root, payload.uid)?.tab;
    expect(tab && 'request' in tab && tab.request.requestType).toBe('grpc');
    expect(tab?.source?.path).toBe('say-hello.yml');
  });

  it('does not show the HTTP method select for gRPC', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);
    await chooseGrpc();
    expect(screen.queryByText('HTTP Method')).toBeNull();
  });
});
