import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ResponseState } from '@/types/pane-types';

const save = vi.fn();
const writeFile = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: (...a: unknown[]) => save(...a) }));
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: (...a: unknown[]) => writeFile(...a) }));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { BinaryResponsePanel } from '../BinaryResponsePanel';

function response(overrides: Partial<ResponseState> = {}): ResponseState {
  return {
    status: 200,
    statusText: 'OK',
    headers: [{ id: '1', key: 'Content-Type', value: 'image/png', enabled: true }],
    body: '',
    durationMs: 1,
    ttfbMs: 1,
    sizeBytes: 4,
    activeView: 'pretty',
    isBinary: true,
    bodyBase64: 'iVBORw==',
    ...overrides,
  };
}

describe('BinaryResponsePanel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('previews an image from a data url', () => {
    render(<BinaryResponsePanel response={response()} sizeLabel='4 B' />);
    const img = screen.getByAltText('Response preview') as HTMLImageElement;
    expect(img.src).toBe('data:image/png;base64,iVBORw==');
  });

  it('offers no preview for a pdf but still offers a save', () => {
    render(
      <BinaryResponsePanel
        response={response({
          headers: [{ id: '1', key: 'Content-Type', value: 'application/pdf', enabled: true }],
        })}
        sizeLabel='4 B'
      />,
    );
    expect(screen.queryByAltText('Response preview')).toBeNull();
    expect(screen.getByRole('button', { name: /save to file/i })).toBeEnabled();
  });

  it('writes the exact bytes to the chosen path', async () => {
    save.mockResolvedValue('/tmp/out.png');
    render(<BinaryResponsePanel response={response()} sizeLabel='4 B' />);
    fireEvent.click(screen.getByRole('button', { name: /save to file/i }));
    await waitFor(() => expect(writeFile).toHaveBeenCalledTimes(1));
    const [path, bytes] = writeFile.mock.calls[0];
    expect(path).toBe('/tmp/out.png');
    expect(Array.from(bytes as Uint8Array)).toEqual([0x89, 0x50, 0x4e, 0x47]);
  });

  it('does not write when the dialog is cancelled', async () => {
    save.mockResolvedValue(null);
    render(<BinaryResponsePanel response={response()} sizeLabel='4 B' />);
    fireEvent.click(screen.getByRole('button', { name: /save to file/i }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(writeFile).not.toHaveBeenCalled();
  });

  it('says so when the body is too large to carry', () => {
    render(
      <BinaryResponsePanel response={response({ bodyBase64: undefined })} sizeLabel='40.0 MB' />,
    );
    expect(screen.getByText(/too large/i)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /save to file/i })).toBeNull();
  });
});
