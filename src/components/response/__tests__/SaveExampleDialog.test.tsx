import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ComponentProps } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ExampleSnapshot } from '@/lib/example-snapshot';
import { addRequestExample } from '@/lib/tauri-api';
import { SaveExampleDialog } from '../SaveExampleDialog';

vi.mock('@/lib/tauri-api', () => ({ addRequestExample: vi.fn() }));
vi.mock('sonner', () => ({ toast: { success: vi.fn() } }));

const snapshot: ExampleSnapshot = {
  request: { url: '/login', method: 'POST' },
  response: { status: 200, statusText: 'OK' },
  notes: [],
};

function renderDialog(extra: Partial<ComponentProps<typeof SaveExampleDialog>> = {}) {
  const onOpenChange = vi.fn();
  render(
    <SaveExampleDialog
      open
      onOpenChange={onOpenChange}
      collection='shop'
      requestPath='auth/login.yml'
      snapshot={snapshot}
      defaultName='200 OK'
      existingNames={[]}
      {...extra}
    />,
  );
  return onOpenChange;
}

describe('SaveExampleDialog', () => {
  beforeEach(() => {
    vi.mocked(addRequestExample).mockReset();
    vi.mocked(addRequestExample).mockResolvedValue(undefined);
  });

  it('shows_default_name_and_saves', async () => {
    const onOpenChange = renderDialog();
    const input = screen.getByLabelText('Example name') as HTMLInputElement;
    expect(input.value).toBe('200 OK');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(addRequestExample).toHaveBeenCalledWith('shop', 'auth/login.yml', {
      name: '200 OK',
      request: snapshot.request,
      response: snapshot.response,
    });
  });

  it('blocks_empty_and_duplicate_names', async () => {
    renderDialog({ existingNames: ['Ok Case'] });
    const input = screen.getByLabelText('Example name');
    await userEvent.clear(input);
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(true);
    await userEvent.type(input, 'ok case');
    expect(screen.getByText('An example with this name already exists.')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(true);
    expect(addRequestExample).not.toHaveBeenCalled();
  });

  it('shows_backend_error_and_stays_open', async () => {
    vi.mocked(addRequestExample).mockRejectedValue('disk is full');
    const onOpenChange = renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('disk is full')).toBeTruthy();
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it('lists_notes', () => {
    renderDialog({ snapshot: { ...snapshot, notes: ['First note.', 'Second note.'] } });
    expect(screen.getByText('First note.')).toBeTruthy();
    expect(screen.getByText('Second note.')).toBeTruthy();
  });

  it('renders_every_note_once', () => {
    renderDialog({
      snapshot: {
        ...snapshot,
        notes: ['Unsaved note.', 'Disabled note.', 'Secrets note.'],
      },
    });
    expect(screen.getAllByRole('listitem')).toHaveLength(3);
  });
});
