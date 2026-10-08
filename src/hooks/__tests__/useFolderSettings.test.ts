import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import { useFolderSettings } from '../useFolderSettings';

const { mockGet, mockSave, mockToastError } = vi.hoisted(() => ({
  mockGet: vi.fn(),
  mockSave: vi.fn(),
  mockToastError: vi.fn(),
}));

vi.mock('@/lib/tauri-api', () => ({
  getFolderSettings: mockGet,
  saveFolderSettings: mockSave,
}));

vi.mock('sonner', () => ({ toast: { error: mockToastError } }));

const base: FolderSettings = { headers: [], variables: [], docs: 'base docs' };

function load(collection = 'col', folderPath = 'a/b') {
  return renderHook(({ c, p }) => useFolderSettings(c, p), {
    initialProps: { c: collection, p: folderPath },
  });
}

beforeEach(() => {
  mockGet.mockReset();
  mockSave.mockReset();
  mockToastError.mockReset();
  mockGet.mockResolvedValue(base);
  mockSave.mockResolvedValue(undefined);
});

describe('useFolderSettings', () => {
  it('loads the settings on mount', async () => {
    const { result } = load();
    expect(result.current.isLoaded).toBe(false);
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    expect(mockGet).toHaveBeenCalledWith('col', 'a/b');
    expect(result.current.settings).toEqual(base);
    expect(result.current.isDirty).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('ignores a stale load response after the folder changes', async () => {
    const a = createDeferred<FolderSettings>();
    const b = createDeferred<FolderSettings>();
    mockGet.mockImplementation((_c: string, path: string) =>
      path === 'a' ? a.promise : b.promise,
    );
    const { result, rerender } = load('col', 'a');
    rerender({ c: 'col', p: 'b' });
    await act(async () => {
      b.resolve({ ...base, docs: 'docs of b' });
    });
    await act(async () => {
      a.resolve({ ...base, docs: 'docs of a' });
    });
    expect(result.current.settings.docs).toBe('docs of b');
    expect(result.current.isLoaded).toBe(true);
  });

  it('ignores edits until the settings are loaded', () => {
    const pending = createDeferred<FolderSettings>();
    mockGet.mockReturnValue(pending.promise);
    const { result } = load();
    act(() => result.current.setSettings({ ...base, docs: 'early' }));
    expect(result.current.isDirty).toBe(false);
    expect(result.current.settings.docs).not.toBe('early');
  });

  it('keeps isLoaded false and reports an error when the load fails', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mockGet.mockRejectedValue(new Error('boom'));
    const { result } = load();
    await waitFor(() => expect(result.current.error).not.toBeNull());
    expect(result.current.isLoaded).toBe(false);
    act(() => result.current.setSettings({ ...base, docs: 'x' }));
    await act(async () => {
      await result.current.save();
    });
    expect(mockSave).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it('save writes the whole object and clears dirty', async () => {
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'new' })));
    expect(result.current.isDirty).toBe(true);
    await act(async () => {
      await result.current.save();
    });
    expect(mockSave).toHaveBeenCalledTimes(1);
    expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'new' });
    expect(result.current.isDirty).toBe(false);
    expect(result.current.saveState).toBe('success');
  });

  it('does not call the backend when nothing changed', async () => {
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    await act(async () => {
      await result.current.save();
    });
    expect(mockSave).not.toHaveBeenCalled();
  });

  it('an edit made during a save keeps the tab dirty', async () => {
    const inflight = createDeferred<void>();
    mockSave.mockReturnValue(inflight.promise);
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'one' })));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.save();
    });
    expect(result.current.saveState).toBe('saving');
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'two' })));
    await act(async () => {
      inflight.resolve();
      await pending;
    });
    expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'one' });
    expect(result.current.isDirty).toBe(true);
    expect(result.current.settings.docs).toBe('two');
  });

  it('a save finishing after the folder changed does not clean the new folder', async () => {
    const inflight = createDeferred<void>();
    mockSave.mockReturnValue(inflight.promise);
    const { result, rerender } = load('col', 'a');
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'edit in a' })));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.save();
    });
    rerender({ c: 'col', p: 'b' });
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'edit in b' })));
    await act(async () => {
      inflight.resolve();
      await pending;
    });
    expect(result.current.isDirty).toBe(true);
    expect(result.current.saveState).toBe('idle');
  });

  it('a save that resolves after unmount is harmless', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const inflight = createDeferred<void>();
    mockSave.mockReturnValue(inflight.promise);
    const { result, unmount } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'late' })));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.save();
    });
    unmount();
    await act(async () => {
      inflight.resolve();
      await pending;
    });
    expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'late' });
    expect(errSpy).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it('a failed save keeps the tab dirty and shows a toast', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mockSave.mockRejectedValue(new Error('disk full'));
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'x' })));
    await act(async () => {
      await result.current.save();
    });
    expect(result.current.isDirty).toBe(true);
    expect(result.current.saveState).toBe('idle');
    expect(mockToastError).toHaveBeenCalledWith('Failed to save folder settings');
    errSpy.mockRestore();
  });

  it('reads the current file on save and keeps fields the tab did not edit', async () => {
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'mine' })));
    mockGet.mockResolvedValue({ ...base, testsScript: 'changed on disk' });
    await act(async () => {
      await result.current.save();
    });
    expect(mockSave).toHaveBeenCalledWith('col', 'a/b', {
      ...base,
      testsScript: 'changed on disk',
      docs: 'mine',
    });
  });
  describe('applyEdits through save', () => {
    const header = { key: 'X', value: '1', enabled: true };

    async function loadWith(initial: FolderSettings) {
      mockGet.mockResolvedValue(initial);
      const hook = load();
      await waitFor(() => expect(hook.result.current.isLoaded).toBe(true));
      return hook;
    }

    it('does not write an array edited and reverted to an equal new reference', async () => {
      const { result } = await loadWith({ ...base, headers: [header] });
      act(() => result.current.setSettings((prev) => ({ ...prev, headers: [] })));
      act(() => result.current.setSettings((prev) => ({ ...prev, headers: [{ ...header }] })));
      const onDisk = { key: 'Y', value: '2', enabled: true };
      mockGet.mockResolvedValue({ ...base, headers: [onDisk], docs: 'disk docs' });
      await act(async () => {
        await result.current.save();
      });
      expect(mockSave).toHaveBeenCalledWith('col', 'a/b', {
        ...base,
        headers: [onDisk],
        docs: 'disk docs',
      });
    });

    it('writes a field cleared to an empty array or string', async () => {
      const { result } = await loadWith({ ...base, headers: [header], docs: 'text' });
      act(() => result.current.setSettings((prev) => ({ ...prev, headers: [], docs: '' })));
      mockGet.mockResolvedValue({ ...base, headers: [header], docs: 'text' });
      await act(async () => {
        await result.current.save();
      });
      expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, headers: [], docs: '' });
    });

    it('deletes the key when a field is set to undefined', async () => {
      const { result } = await loadWith({ ...base, testsScript: 'old' });
      act(() => result.current.setSettings((prev) => ({ ...prev, testsScript: undefined })));
      mockGet.mockResolvedValue({ ...base, testsScript: 'old' });
      await act(async () => {
        await result.current.save();
      });
      const written = mockSave.mock.calls[0][2] as Record<string, unknown>;
      expect('testsScript' in written).toBe(false);
    });

    it('keeps external changes to unedited fields and lets an edited field win', async () => {
      const { result } = await loadWith(base);
      act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'mine' })));
      mockGet.mockResolvedValue({
        ...base,
        docs: 'disk docs',
        preRequestScript: 'disk script',
      });
      await act(async () => {
        await result.current.save();
      });
      expect(mockSave).toHaveBeenCalledWith('col', 'a/b', {
        ...base,
        docs: 'mine',
        preRequestScript: 'disk script',
      });
    });
  });
});
