import { afterEach, describe, expect, it, vi } from 'vitest';
import { copyTextAsync } from '@/lib/clipboard';

class FakeClipboardItem {
  constructor(public items: Record<string, Promise<Blob>>) {}
}

afterEach(() => {
  vi.unstubAllGlobals();
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
});
