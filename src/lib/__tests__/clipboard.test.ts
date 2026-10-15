import { afterEach, describe, expect, it, vi } from 'vitest';
import { copyTextAsync } from '@/lib/clipboard';

const tauri = vi.hoisted(() => ({ isTauri: false, writeText: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => tauri.isTauri }));
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ writeText: tauri.writeText }));

class FakeClipboardItem {
  constructor(public items: Record<string, Promise<Blob>>) {}
}

afterEach(() => {
  vi.unstubAllGlobals();
  tauri.isTauri = false;
  tauri.writeText.mockReset();
});

describe('copyTextAsync', () => {
  it('starts the clipboard write before the text resolves', async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('ClipboardItem', FakeClipboardItem);
    vi.stubGlobal('navigator', { clipboard: { write, writeText: vi.fn() } });

    let resolveText: ((v: string) => void) | undefined;
    const pending = new Promise<string>((r) => {
      resolveText = r;
    });
    const done = copyTextAsync(pending);
    expect(write).toHaveBeenCalledTimes(1);
    resolveText?.('curl x');
    await done;
    const item = write.mock.calls[0][0][0] as FakeClipboardItem;
    expect(await (await item.items['text/plain']).text()).toBe('curl x');
  });

  it('falls back to writeText when write is rejected', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('ClipboardItem', FakeClipboardItem);
    vi.stubGlobal('navigator', {
      clipboard: { write: vi.fn().mockRejectedValue(new Error('denied')), writeText },
    });
    vi.spyOn(console, 'warn').mockReturnValue(undefined);
    await copyTextAsync(Promise.resolve('abc'));
    expect(writeText).toHaveBeenCalledWith('abc');
  });

  it('starts the web write first inside Tauri, so the click activation is still valid', async () => {
    tauri.isTauri = true;
    const write = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('ClipboardItem', FakeClipboardItem);
    vi.stubGlobal('navigator', { clipboard: { write, writeText: vi.fn() } });

    const done = copyTextAsync(Promise.resolve('x'));
    expect(write).toHaveBeenCalledTimes(1);
    await done;
    expect(tauri.writeText).not.toHaveBeenCalled();
  });

  it('falls back to the native plugin inside Tauri when the web write is rejected', async () => {
    tauri.isTauri = true;
    tauri.writeText.mockResolvedValue(undefined);
    vi.stubGlobal('ClipboardItem', FakeClipboardItem);
    vi.stubGlobal('navigator', {
      clipboard: { write: vi.fn().mockRejectedValue(new Error('NotAllowed')), writeText: vi.fn() },
    });
    vi.spyOn(console, 'warn').mockReturnValue(undefined);
    await copyTextAsync(Promise.resolve('curl y'));
    expect(tauri.writeText).toHaveBeenCalledWith('curl y');
  });

  it('uses writeText when the web write and the native plugin both fail', async () => {
    tauri.isTauri = true;
    tauri.writeText.mockRejectedValue(new Error('X11 server connection timed out'));
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('ClipboardItem', FakeClipboardItem);
    vi.stubGlobal('navigator', {
      clipboard: { write: vi.fn().mockRejectedValue(new Error('NotAllowed')), writeText },
    });
    vi.spyOn(console, 'warn').mockReturnValue(undefined);
    await copyTextAsync(Promise.resolve('curl z'));
    expect(writeText).toHaveBeenCalledWith('curl z');
  });
});
