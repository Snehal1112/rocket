import { beforeEach, describe, expect, it, vi } from 'vitest';

const save = vi.fn();
const writeFile = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: (...a: unknown[]) => save(...a) }));
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: (...a: unknown[]) => writeFile(...a) }));

import { saveTextFile } from '../save-file';

const filters = [{ name: 'JSON', extensions: ['json'] }];

describe('saveTextFile', () => {
  beforeEach(() => {
    save.mockReset();
    writeFile.mockReset();
  });

  it('asks for a path with the default name and filters, then writes the text as UTF-8', async () => {
    save.mockResolvedValue('/tmp/out.json');
    writeFile.mockResolvedValue(undefined);

    const saved = await saveTextFile('report.json', 'héllo', filters);

    expect(saved).toBe(true);
    expect(save).toHaveBeenCalledWith({ defaultPath: 'report.json', filters });
    expect(writeFile).toHaveBeenCalledTimes(1);
    const [path, bytes] = writeFile.mock.calls[0] as [string, Uint8Array];
    expect(path).toBe('/tmp/out.json');
    expect(new TextDecoder().decode(bytes)).toBe('héllo');
  });

  it('writes nothing and returns false when the dialog is cancelled', async () => {
    save.mockResolvedValue(null);
    expect(await saveTextFile('r.json', 'x', filters)).toBe(false);
    expect(writeFile).not.toHaveBeenCalled();
  });

  it('rejects when the write fails', async () => {
    save.mockResolvedValue('/tmp/out.json');
    writeFile.mockRejectedValue(new Error('disk full'));
    await expect(saveTextFile('r.json', 'x', filters)).rejects.toThrow('disk full');
  });
});
