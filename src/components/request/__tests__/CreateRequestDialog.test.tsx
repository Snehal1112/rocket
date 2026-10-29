import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { CreateRequestDialog } from '@/components/request/CreateRequestDialog';
import { createDefaultLeaf } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn(),
    saveGraphQlRequest: vi.fn(),
    createGraphQlRequest: vi.fn(),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

async function chooseGraphQl() {
  await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
  await userEvent.click(await screen.findByRole('option', { name: 'GraphQL' }));
}

describe('CreateRequestDialog graphql', () => {
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
    vi.mocked(tauriApi.saveGraphQlRequest).mockReset();
    vi.mocked(tauriApi.createGraphQlRequest).mockReset();
    vi.mocked(toast.error).mockReset();
  });

  it('creates through the exclusive wrapper, not the save wrapper', async () => {
    vi.mocked(tauriApi.createGraphQlRequest).mockImplementation(async (_c, _p, request) => ({
      ...request,
      fileName: 'users.yml',
    }));
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await chooseGraphQl();
    await userEvent.type(screen.getByLabelText('Request Name'), 'users');
    await userEvent.type(screen.getByLabelText('URL'), 'https://api.example.com/graphql');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.createGraphQlRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveGraphQlRequest).not.toHaveBeenCalled();
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [, , payload] = vi.mocked(tauriApi.createGraphQlRequest).mock.calls[0];
    expect(payload.method).toBe('POST');
    expect(payload.body.query).toContain('__typename');
  });

  it('shows the already exists toast and opens no tab', async () => {
    vi.mocked(tauriApi.createGraphQlRequest).mockRejectedValue(
      new Error('Already exists: col/foo.yml'),
    );
    const initialRoot = usePaneStore.getState().root;
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await chooseGraphQl();
    await userEvent.type(screen.getByLabelText('Request Name'), 'foo');
    await userEvent.type(screen.getByLabelText('URL'), 'https://api.example.com/graphql');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        'A request named "foo" already exists here. Pick another name.',
      ),
    );
    expect(usePaneStore.getState().root).toEqual(initialRoot);
  });
});
