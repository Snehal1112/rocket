import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
  };
});

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
  });

  it('saves a real GraphQL item, not an HTTP request tagged graphql', async () => {
    vi.mocked(tauriApi.saveGraphQlRequest).mockImplementation(async (_c, _p, request) => ({
      ...request,
      fileName: 'users.yml',
    }));
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
    await userEvent.click(await screen.findByRole('option', { name: 'GraphQL' }));
    await userEvent.type(screen.getByLabelText('Request Name'), 'users');
    await userEvent.type(screen.getByLabelText('URL'), 'https://api.example.com/graphql');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.saveGraphQlRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [, , payload] = vi.mocked(tauriApi.saveGraphQlRequest).mock.calls[0];
    expect(payload.method).toBe('POST');
    expect(payload.body.query).toContain('__typename');
  });
});
