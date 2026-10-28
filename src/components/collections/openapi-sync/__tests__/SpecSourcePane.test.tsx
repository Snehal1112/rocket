import { act, cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { editorOptions, LARGE_SOURCE_BYTES, SpecSourcePane } from '../SpecSourcePane';

vi.mock('@/components/editor/monaco-setup', () => ({}));
vi.mock('@/components/editor/useMonacoTheme', () => ({
  useMonacoTheme: () => ({ themeName: 'rocket-light', defineThemes: vi.fn(), isDark: false }),
}));

const mocks = vi.hoisted(() => ({
  clear: vi.fn(),
  editor: {
    revealLineInCenter: vi.fn(),
    setPosition: vi.fn(),
    createDecorationsCollection: vi.fn(),
  },
}));

vi.mock('@monaco-editor/react', async () => {
  const { useEffect } = await import('react');
  return {
    default: ({ onMount }: { onMount?: (editor: typeof mocks.editor) => void }) => {
      // biome-ignore lint/correctness/useExhaustiveDependencies: Mount only.
      useEffect(() => onMount?.(mocks.editor), []);
      return <div data-testid='spec-editor' />;
    },
  };
});

const source = { text: 'a: 1', format: 'yaml' as const, sizeBytes: 10, tooLarge: false };

describe('editorOptions', () => {
  it('keeps_the_extras_below_the_threshold', () => {
    const o = editorOptions(LARGE_SOURCE_BYTES - 1);
    expect(o.minimap).toEqual({ enabled: true });
    expect(o.matchBrackets).toBeUndefined();
    expect(o.stickyScroll).toBeUndefined();
    expect(o.folding).toBe(true);
    expect(o.lineNumbers).toBe('on');
  });

  it('turns_the_costly_extras_off_above_the_threshold', () => {
    const o = editorOptions(LARGE_SOURCE_BYTES);
    expect(o.minimap).toEqual({ enabled: false });
    expect(o.bracketPairColorization).toEqual({ enabled: false });
    expect(o.matchBrackets).toBe('never');
    expect(o.occurrencesHighlight).toBe('off');
    expect(o.selectionHighlight).toBe(false);
    expect(o.links).toBe(false);
    expect(o.colorDecorators).toBe(false);
    expect(o.guides).toEqual({ indentation: false, bracketPairs: false });
    expect(o.renderLineHighlight).toBe('none');
    expect(o.stickyScroll).toEqual({ enabled: false });
    expect(o.folding).toBe(true);
    expect(o.lineNumbers).toBe('on');
  });
});

describe('SpecSourcePane flash', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.clear.mockClear();
    mocks.editor.createDecorationsCollection.mockReset();
    mocks.editor.createDecorationsCollection.mockImplementation(() => ({ clear: mocks.clear }));
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  // The mount and the reveal effect can both show the line, so counts are taken after the mount.
  it('the_flash_clears_after_its_time', () => {
    render(<SpecSourcePane source={source} reveal={{ line: 3, nonce: 1 }} />);
    expect(mocks.editor.createDecorationsCollection).toHaveBeenCalled();
    const before = mocks.clear.mock.calls.length;
    act(() => {
      vi.advanceTimersByTime(1600);
    });
    expect(mocks.clear).toHaveBeenCalledTimes(before + 1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('unmounting_clears_the_flash_and_its_timer', () => {
    const { unmount } = render(<SpecSourcePane source={source} reveal={{ line: 3, nonce: 1 }} />);
    const before = mocks.clear.mock.calls.length;
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(mocks.clear).toHaveBeenCalledTimes(before + 1);
    expect(vi.getTimerCount()).toBe(0);
  });
});
