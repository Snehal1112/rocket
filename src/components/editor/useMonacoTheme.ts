import { useEffect, useState } from 'react';
import type { Monaco } from '@monaco-editor/react';

export function useMonacoTheme() {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  );

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mq.addEventListener('change', handler);

    // Also observe the document class for manual dark mode toggle.
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
    monaco.editor.defineTheme('rocket-light', {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'string', foreground: '0d9488' },   // teal
        { token: 'number', foreground: 'd97706' },   // amber
        { token: 'keyword', foreground: '7c3aed' },  // purple
        { token: 'comment', foreground: '9ca3af' },  // gray
        { token: 'type', foreground: '2563eb' },     // blue
      ],
      colors: {
        'editor.background': '#faf9f8',
        'editor.foreground': '#1c1917',
        'editor.lineHighlightBackground': '#f5f5f4',
        'editorLineNumber.foreground': '#a8a29e',
      },
    });

    monaco.editor.defineTheme('rocket-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'string', foreground: '5eead4' },   // teal-light
        { token: 'number', foreground: 'fbbf24' },   // amber-light
        { token: 'keyword', foreground: 'a78bfa' },  // purple-light
        { token: 'comment', foreground: '6b7280' },  // gray
        { token: 'type', foreground: '60a5fa' },     // blue-light
      ],
      colors: {
        'editor.background': '#1a1816',
        'editor.foreground': '#e7e5e4',
        'editor.lineHighlightBackground': '#292524',
        'editorLineNumber.foreground': '#78716c',
      },
    });
  }

  return { themeName, defineThemes, isDark };
}
