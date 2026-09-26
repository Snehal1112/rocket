import { render } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { MonacoWrapper } from '../MonacoWrapper';

// vi.hoisted is required here (rather than plain top-level consts) because
// MonacoWrapper is imported statically below and itself imports
// monaco-js-worker-lifecycle; ES module imports execute before ordinary
// top-level statements, so a plain `const acquireJsWorker = vi.fn()` would
// still be in its temporal dead zone when the mock factory runs.
const { acquireJsWorker, releaseJsWorker } = vi.hoisted(() => ({
  acquireJsWorker: vi.fn(),
  releaseJsWorker: vi.fn(),
}));
vi.mock('../monaco-js-worker-lifecycle', () => ({ acquireJsWorker, releaseJsWorker }));

// jsdom does not implement window.matchMedia. useMonacoTheme calls it to track OS dark-mode changes.
// (Same setup as DiffViewer.test.tsx, which hits the same environment gap.)
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

// Monaco cannot render in jsdom (hits browser-only APIs at import time).
vi.mock('../monaco-setup', () => ({}));

// MonacoWrapper imports `monaco-editor` at runtime (not type-only) to call
// `monacoNs.typescript.javascriptDefaults.addExtraLib` in its phase effect,
// so the real package must be stubbed too, not just @monaco-editor/react.
vi.mock('monaco-editor', () => ({
  typescript: {
    javascriptDefaults: {
      addExtraLib: vi.fn(),
      setCompilerOptions: vi.fn(),
      getCompilerOptions: vi.fn(() => ({})),
    },
  },
}));

// @monaco-editor/react's <Editor> needs a DOM measurement environment it
// doesn't have in jsdom by default — stub it to a plain div so this test
// only exercises MonacoWrapper's own lifecycle effect, not Monaco itself.
vi.mock('@monaco-editor/react', () => ({
  default: () => <div data-testid='editor-stub' />,
  loader: {
    init: vi.fn().mockResolvedValue({
      editor: {
        defineTheme: vi.fn(),
        setTheme: vi.fn(),
      },
    }),
  },
}));

describe('MonacoWrapper JS worker lifecycle', () => {
  beforeEach(() => {
    acquireJsWorker.mockClear();
    releaseJsWorker.mockClear();
  });

  it('acquires the JS worker on mount for language="javascript" and releases on unmount', () => {
    const { unmount } = render(<MonacoWrapper value='' language='javascript' />);
    expect(acquireJsWorker).toHaveBeenCalledTimes(1);
    expect(releaseJsWorker).not.toHaveBeenCalled();

    unmount();
    expect(releaseJsWorker).toHaveBeenCalledTimes(1);
  });

  it('does not acquire the JS worker for a non-JS language', () => {
    render(<MonacoWrapper value='' language='json' />);
    expect(acquireJsWorker).not.toHaveBeenCalled();
  });
});
