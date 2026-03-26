import { useEffect, useState } from 'react';
import type { Monaco } from '@monaco-editor/react';

// Reads a CSS variable from the document root.
function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

export function useMonacoTheme() {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  );

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mq.addEventListener('change', handler);

    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    return () => {
      mq.removeEventListener('change', handler);
      observer.disconnect();
    };
  }, []);

  const themeName = isDark ? 'rocket-dark' : 'rocket-light';

  function defineThemes(monaco: Monaco) {
    const bg = cssVar('--monaco-bg');
    const fg = cssVar('--monaco-fg');
    const lineHighlight = cssVar('--monaco-line-highlight');
    const lineNumber = cssVar('--monaco-line-number');
    const str = cssVar('--monaco-string');
    const num = cssVar('--monaco-number');
    const kw = cssVar('--monaco-keyword');
    const comment = cssVar('--monaco-comment');
    const type = cssVar('--monaco-type');

    monaco.editor.defineTheme('rocket-light', {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'string', foreground: str },
        { token: 'number', foreground: num },
        { token: 'keyword', foreground: kw },
        { token: 'comment', foreground: comment },
        { token: 'type', foreground: type },
      ],
      colors: {
        'editor.background': bg,
        'editor.foreground': fg,
        'editor.lineHighlightBackground': lineHighlight,
        'editorLineNumber.foreground': lineNumber,
      },
    });

    monaco.editor.defineTheme('rocket-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'string', foreground: str },
        { token: 'number', foreground: num },
        { token: 'keyword', foreground: kw },
        { token: 'comment', foreground: comment },
        { token: 'type', foreground: type },
      ],
      colors: {
        'editor.background': bg,
        'editor.foreground': fg,
        'editor.lineHighlightBackground': lineHighlight,
        'editorLineNumber.foreground': lineNumber,
      },
    });
  }

  return { themeName, defineThemes, isDark };
}
